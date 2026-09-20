import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import { writeTextFile, readTextFile } from "@tauri-apps/plugin-fs";
import { toast } from "svelte-sonner";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import {
    deserialize_column,
    get_column_id,
    set_column_id,
    type Column,
    type ColumnSerialized,
    type ColumnSort,
} from "./type/column.svelte";
import {
    type Task,
    clone_task,
    duplicate_task as create_task_duplicate,
    get_task_id,
    set_task_id,
    serialize_task,
    deserialize_task,
    type TaskSerialized,
    type TaskTemplate,
    type TaskTemplateSerialized,
} from "./type/task.svelte";
import type { Archive } from "./type/archive.svelte";
import { parse_import_summary, type ImportSummary } from "./utils/import-data";
import { addMission } from "./utils/mission.svelte";

// Keep drag-and-drop responsive by waiting until the user has finished a
// short sequence of moves before persisting it to disk.
const DRAG_PERSIST_DEBOUNCE_MS = 300;
const COLUMN_FETCH_TIMEOUT_MS = 15_000;

function with_timeout<T>(request: Promise<T>, timeout_ms: number, message: string): Promise<T> {
    let timeout: ReturnType<typeof setTimeout> | undefined;
    return new Promise<T>((resolve, reject) => {
        timeout = setTimeout(() => reject(new Error(message)), timeout_ms);
        request.then(resolve, reject).finally(() => {
            if (timeout !== undefined) clearTimeout(timeout);
        });
    });
}

export interface ImportCandidate {
    json_data: string;
    summary: ImportSummary;
    board_name: string;
}

interface CapturedTaskId {
    was_pending: boolean;
    id: Promise<number>;
}

export interface BoardSummary { id: number; name: string; task_count: number; }
interface BoardsState { boards: BoardSummary[]; active_board_id: number; }

// ============ Board Store (class-based for Svelte 5 rune compatibility) ============

export class BoardStore {
    // Reactive state
    columns = $state<Column[]>([]);
    labels = $state<string[]>([]);
    archives = $state<Archive[]>([]);
    archives_loaded = $state(false);
    archives_loading = $state(false);
    expired_tasks = $state<Task[]>([]);
    templates = $state<TaskTemplate[]>([]);
    column_fetch_finish = $state(false);
    column_fetch_error = $state(false);
    notify_enabled = $state(false);
    notification_setting_updating = $state(false);
    can_undo = $state(false);
    undo_in_progress = $state(false);
    boards = $state<BoardSummary[]>([]);
    active_board_id = $state<number | null>(null);
    private board_generation = 0;
    private expired_task_checker: ReturnType<typeof setInterval> | undefined;
    private task_move_timers = new Map<string, ReturnType<typeof setTimeout>>();
    private column_move_timers = new Map<string, ReturnType<typeof setTimeout>>();
    private pending_task_creations = new Map<string, Promise<string>>();
    // Dialogs and delayed callbacks can retain a temporary ID after creation
    // has completed, so keep a session-scoped alias to its persisted ID.
    private resolved_task_ids = new Map<string, string>();
    private archives_request: Promise<void> | undefined;

    // ============ Data Fetching ============

    private show_data_fetch_error(message: string, retry: () => void) {
        toast.error(message, {
            action: {
                label: "Retry",
                onClick: retry,
            },
        });
    }

    private show_success_with_undo(message: string) {
        this.can_undo = true;
        toast.success(message, {
            action: {
                label: "Undo",
                onClick: () => void this.undo(),
            },
        });
    }

    async undo(): Promise<boolean> {
        if (!this.can_undo || this.undo_in_progress) return false;
        this.undo_in_progress = true;
        try {
            const expectedBoardId = this.active_board_id;
            const generation = this.board_generation;
            if (expectedBoardId === null) return false;
            const canUndo = await addMission(() => invoke<boolean>("undo", { expectedBoardId }));
            if (generation !== this.board_generation) return false;
            this.can_undo = canUndo;
            this.get_columns();
            this.refresh_archives_if_loaded();
            this.get_task_templates();
            this.get_expired_tasks();
            this.update_labels();
            await this.load_settings();
            toast.success("Last action undone");
            return true;
        } catch (e: unknown) {
            console.log(e);
            toast.error("Couldn't undo the last action");
            return false;
        } finally {
            this.undo_in_progress = false;
        }
    }

    private refresh_labels_from_columns() {
        this.sync_active_board_task_count();
        const active_labels = new Set(
            this.columns.flatMap((column) =>
                column.tasks.flatMap((task) => task.labels),
            ),
        );
        this.labels = [
            ...this.labels.filter((label) => active_labels.delete(label)),
            ...Array.from(active_labels).sort(),
        ];
    }

    update_labels() {
        const board = this;
        const expectedBoardId=this.active_board_id; const generation=this.board_generation;if(expectedBoardId===null)return;
        addMission(() => invoke<string[]>("get_labels",{expectedBoardId}))
            .then((data) => {
                if(generation!==board.board_generation)return;
                board.labels = data;
            })
            .catch((e) => {
                console.log(e);
                board.show_data_fetch_error("Couldn't load labels", () => board.update_labels());
            });
    }

    get_columns() {
        const board = this;
        const expectedBoardId=this.active_board_id; const generation=this.board_generation;if(expectedBoardId===null)return;
        // Only replace the workspace with the loading state during the initial fetch
        // or when retrying an initial-load error.
        // Later refreshes (for example after archiving a recurring task) must keep the
        // existing view mounted so its scroll position is preserved.
        const replaces_workspace = !board.column_fetch_finish || board.column_fetch_error;
        if (replaces_workspace) {
            board.column_fetch_finish = false;
            board.column_fetch_error = false;
        }
        // The main workspace readiness gate must not wait behind unrelated
        // background missions (such as loading labels). If one of those IPC
        // calls never settles, queueing this read leaves the app on its
        // initial loading screen forever.
        with_timeout(
            invoke<ColumnSerialized[]>("get_columns",{expectedBoardId}),
            COLUMN_FETCH_TIMEOUT_MS,
            "Loading board data timed out",
        )
            .then((data: ColumnSerialized[]) => {
                if(generation!==board.board_generation)return;
                board.columns = data.map(deserialize_column);
                board.sync_active_board_task_count();
                board.column_fetch_finish = true;
            })
            .catch((e) => {
                console.log(e);
                if (replaces_workspace) {
                    board.column_fetch_error = true;
                    board.column_fetch_finish = true;
                } else {
                    board.show_data_fetch_error("Couldn't refresh columns", () =>
                        board.get_columns(),
                    );
                }
            });
    }

    get_archives() {
        if (this.archives_request) return this.archives_request;
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve();
        board.archives_loading = true;
        const request = addMission(() =>
            invoke<{ time: number; task: TaskSerialized }[]>("get_archives",{expectedBoardId}),
        )
            .then((data) => {
                if (generation !== board.board_generation) return;
                board.archives = data.reduce<Archive[]>((list, d) => {
                    list.push({
                        time: new Date(d.time),
                        task: deserialize_task(d.task),
                    });
                    return list;
                }, []);
                board.archives_loaded = true;
            })
            .catch((e) => {
                console.log(e);
                if (generation === board.board_generation) {
                    board.show_data_fetch_error("Couldn't load archives", () => board.get_archives());
                }
            })
            .finally(() => {
                if (generation === board.board_generation) {
                    board.archives_loading = false;
                    board.archives_request = undefined;
                }
            });
        this.archives_request = request;
        return request;
    }

    ensure_archives_loaded() {
        if (this.archives_loaded) return Promise.resolve();
        return this.get_archives();
    }

    private refresh_archives_if_loaded() {
        if (this.archives_loaded) void this.get_archives();
    }

    get_expired_tasks() {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        addMission(() => invoke<TaskSerialized[]>("get_expired_tasks", { expectedBoardId }))
            .then((data) => {
                if (generation !== board.board_generation) return;
                board.expired_tasks = data.map((t) => deserialize_task(t));
            })
            .catch((e) => {
                console.log(e);
                if (generation === board.board_generation) {
                    board.show_data_fetch_error("Couldn't load expired tasks", () =>
                        board.get_expired_tasks(),
                    );
                }
            });
    }

    async init() {
        await this.get_boards();
        this.update_labels();
        this.get_columns();
        this.get_task_templates();

        // Load settings
        await this.load_settings();
        await this.show_recovery_messages();

        // Check notification permission and start expired task checker (only if enabled)
        if (this.notify_enabled) {
            const permission_granted = await this.enable_expired_task_notifications();
            if (!permission_granted) {
                await this.set_notify_enabled(false);
                toast.warning("Notifications disabled because permission wasn't granted");
            }
        }
    }

    private sync_active_board_task_count() {
        const active_board_id = this.active_board_id;
        if (active_board_id === null) return;
        const task_count = this.columns.reduce((total, column) => total + column.tasks.length, 0);
        this.boards = this.boards.map((item) =>
            item.id === active_board_id && item.task_count !== task_count
                ? { ...item, task_count }
                : item,
        );
    }

    async get_boards() {
        try {
            const state = await invoke<BoardsState>("get_boards");
            this.boards = state.boards;
            this.active_board_id = state.active_board_id;
        } catch (e) { console.log("Couldn't load boards:", e); }
    }

    async create_board(name: string) {
        const trimmed = name.trim(); if (!trimmed) return false;
        try { const created = await invoke<BoardSummary>("create_board", { name: trimmed }); this.boards = [...this.boards, created]; await this.switch_board(created.id); return true; }
        catch (e) { console.log(e); toast.error("Couldn't create board"); return false; }
    }

    async rename_board(id: number, name: string) {
        const trimmed = name.trim(); if (!trimmed) return false;
        try { await invoke("rename_board", { boardId: id, name: trimmed }); this.boards = this.boards.map(board => board.id === id ? { ...board, name: trimmed } : board); return true; }
        catch (e) { console.log(e); toast.error("Couldn't rename board"); return false; }
    }

    async switch_board(id: number): Promise<boolean> {
        if (id === this.active_board_id) return true;
        try {
            this.board_generation++; for(const timer of this.task_move_timers.values())clearTimeout(timer);for(const timer of this.column_move_timers.values())clearTimeout(timer);this.task_move_timers.clear();this.column_move_timers.clear();
            await invoke("switch_board", { boardId: id }); this.active_board_id = id;
            this.archives = []; this.archives_loaded = false; this.archives_loading = false; this.archives_request = undefined; this.templates = []; this.labels = []; this.expired_tasks = []; this.can_undo = false;
            this.get_columns(); this.update_labels(); this.get_task_templates(); this.get_expired_tasks();
            return true;
        } catch (e) { console.log(e); toast.error("Couldn't switch board"); return false; }
    }

    async delete_board(id: number) {
        try { const next = await invoke<number>("delete_board", { boardId: id }); this.boards = this.boards.filter(board => board.id !== id); if (id === this.active_board_id) { this.active_board_id = null; await this.switch_board(next); } return true; }
        catch (e) { console.log(e); toast.error("Couldn't delete board"); return false; }
    }

    private async load_settings() {
        try {
            const settings = await invoke<{ notify_enabled: boolean }>("get_settings");
            this.notify_enabled = settings.notify_enabled;
        } catch (e) {
            console.log("Couldn't load settings:", e);
        }
    }

    async set_notify_enabled(enabled: boolean) {
        if (this.notification_setting_updating) return;
        this.notification_setting_updating = true;

        try {
            if (enabled && !(await this.request_notification_permission())) {
                toast.error("Notification permission wasn't granted");
                return;
            }

            const saved = await addMission(async () => {
                try {
                    await invoke<void>("set_notify_enabled", { enabled });
                    return true;
                } catch (e: unknown) {
                    console.log("Couldn't save notification settings:", e);
                    return false;
                }
            });
            if (!saved) {
                toast.error("Couldn't save notification settings");
                return;
            }

            this.notify_enabled = enabled;
            this.can_undo = true;
            if (enabled) {
                this.start_expired_task_checker();
            } else {
                this.stop_expired_task_checker();
            }
        } finally {
            this.notification_setting_updating = false;
        }
    }

    private async request_notification_permission(): Promise<boolean> {
        try {
            let permission_granted = await isPermissionGranted();
            if (!permission_granted) {
                const permission = await requestPermission();
                permission_granted = permission === "granted";
            }
            return permission_granted;
        } catch (e: unknown) {
            console.log("Couldn't request notification permission:", e);
            return false;
        }
    }

    private async enable_expired_task_notifications(): Promise<boolean> {
        const permission_granted = await this.request_notification_permission();
        if (permission_granted && this.notify_enabled) {
            this.start_expired_task_checker();
        }
        return permission_granted;
    }

    private start_expired_task_checker() {
        if (this.expired_task_checker !== undefined) return;

        const check_expired_tasks = async () => {
            if (!this.notify_enabled) return;
            try {
                await invoke("check_expired_tasks");
            } catch (e: unknown) {
                console.log("Couldn't check expired tasks:", e);
            }
        };

        void check_expired_tasks();
        // Check every minute for expired tasks
        this.expired_task_checker = setInterval(() => {
            void check_expired_tasks();
        }, 60000);
    }

    private stop_expired_task_checker() {
        if (this.expired_task_checker === undefined) return;
        clearInterval(this.expired_task_checker);
        this.expired_task_checker = undefined;
    }

    // ============ Task Operations ============

    sort_column_tasks(column: Column) {
        if (column.sort_order === "custom") return;
        const direction = column.sort_order === "due_date_asc" ? 1 : -1;
        column.tasks.sort((a, b) => {
            if (!a.due_time) return b.due_time ? 1 : 0;
            if (!b.due_time) return -1;
            return (a.due_time.getTime() - b.due_time.getTime()) * direction;
        });
    }

    get_task_templates() {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        addMission(() => invoke<TaskTemplateSerialized[]>("get_task_templates",{expectedBoardId}))
            .then((templates) => {
                if (generation !== this.board_generation) return;
                this.templates = templates.map((template) => {
                    const task = deserialize_task(template.task);
                    task.id = "";
                    return { id: template.id, name: template.name, task };
                });
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation === this.board_generation) toast.error("Couldn't load task templates");
            });
    }

    save_task_template(task_id: string, name: string): Promise<boolean> {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        const task_id_resolution = this.capture_task_id(task_id);
        return addMission(async () =>
            invoke<TaskTemplateSerialized>("save_task_template", {
                taskId: await task_id_resolution.id,
                name,
                expectedBoardId,
            }),
        )
            .then((template) => {
                if (generation !== this.board_generation) return false;
                const task = deserialize_task(template.task);
                task.id = "";
                this.templates = [
                    ...this.templates,
                    { id: template.id, name: template.name, task },
                ];
                this.show_success_with_undo("Template saved");
                return true;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation === this.board_generation) toast.error("Couldn't save template");
                return false;
            });
    }

    update_task_template(
        template_id: number,
        name: string,
        task: Task,
    ): Promise<boolean> {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        return addMission(() =>
            invoke<TaskTemplateSerialized>("update_task_template", {
                templateId: template_id,
                name,
                task: serialize_task(task),
                expectedBoardId,
            }),
        )
            .then((template) => {
                if (generation !== this.board_generation) return false;
                const updated_task = deserialize_task(template.task);
                updated_task.id = "";
                this.templates = this.templates.map((candidate) =>
                    candidate.id === template.id
                        ? { id: template.id, name: template.name, task: updated_task }
                        : candidate,
                );
                this.show_success_with_undo("Template updated");
                return true;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation === this.board_generation) toast.error("Couldn't update template");
                return false;
            });
    }

    delete_task_template(template_id: number): Promise<boolean> {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        return addMission(() =>
            invoke<void>("delete_task_template", { templateId: template_id, expectedBoardId }),
        )
            .then(() => {
                if (generation !== this.board_generation) return false;
                this.templates = this.templates.filter(
                    (template) => template.id !== template_id,
                );
                this.show_success_with_undo("Template deleted");
                return true;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation === this.board_generation) toast.error("Couldn't delete template");
                return false;
            });
    }

    private find_task_position(task_id: string) {
        const current_task_id = this.resolve_task_alias(task_id);
        for (let column_idx = 0; column_idx < this.columns.length; column_idx++) {
            const task_idx = this.columns[column_idx].tasks.findIndex(
                (task) => task.id === current_task_id,
            );
            if (task_idx !== -1) return { column_idx, task_idx };
        }
        return undefined;
    }

    private resolve_task_alias(task_id: string): string {
        return this.resolved_task_ids.get(task_id) ?? task_id;
    }

    private capture_task_id(task_id: string): CapturedTaskId {
        const resolved_task_id = this.resolve_task_alias(task_id);
        if (resolved_task_id !== task_id) {
            return {
                was_pending: false,
                id: Promise.resolve(get_task_id(resolved_task_id)),
            };
        }
        const pending_creation = this.pending_task_creations.get(task_id);
        return {
            was_pending: pending_creation !== undefined,
            id: (pending_creation ?? Promise.resolve(task_id)).then((persisted_task_id) =>
                get_task_id(persisted_task_id),
            ),
        };
    }

    private cancel_task_move_timer(task_id: string) {
        const timer = this.task_move_timers.get(task_id);
        if (!timer) return;
        clearTimeout(timer);
        this.task_move_timers.delete(task_id);
    }

    private forget_task_move_timer(timer: ReturnType<typeof setTimeout>) {
        for (const [task_id, candidate] of this.task_move_timers) {
            if (candidate === timer) this.task_move_timers.delete(task_id);
        }
    }

    private rekey_task_move_timer(pending_task_id: string, persisted_task_id: string) {
        const timer = this.task_move_timers.get(pending_task_id);
        if (!timer) return;
        this.task_move_timers.delete(pending_task_id);

        // A move made after creation resolved is newer and should win over a
        // timer that was scheduled while the task still had its temporary ID.
        if (this.task_move_timers.has(persisted_task_id)) {
            clearTimeout(timer);
            return;
        }
        this.task_move_timers.set(persisted_task_id, timer);
    }

    add_new_task(
        column_id: string,
        task: Task,
        after_task_id: string | null = null,
    ): Promise<boolean> {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        const new_task = clone_task(task);

        // Use a temporary ID so the new card can appear before the backend has
        // allocated its persistent ID.
        new_task.id = `pending_task_${globalThis.crypto.randomUUID()}`;
        const pending_task_id = new_task.id;
        const column = this.columns.find((candidate) => candidate.id === column_id);
        if (!column) {
            toast.error("Column no longer exists");
            return Promise.resolve(false);
        }
        const after_task_idx =
            after_task_id === null
                ? -1
                : column.tasks.findIndex(
                      (candidate) => candidate.id === this.resolve_task_alias(after_task_id),
                  );
        if (after_task_id !== null && after_task_idx === -1) {
            toast.error("Original task no longer exists");
            return Promise.resolve(false);
        }
        const after_task_resolution =
            after_task_id === null ? null : this.capture_task_id(after_task_id);
        const insertion_idx =
            after_task_idx === -1 ? column.tasks.length : after_task_idx + 1;
        column.tasks.splice(insertion_idx, 0, new_task);
        this.sort_column_tasks(column);
        this.columns = [...this.columns];
        this.refresh_labels_from_columns();

        const creation = addMission(async () =>
            invoke<number>("add_task", {
                columnId: get_column_id(column_id),
                task: serialize_task(new_task),
                afterTaskId:
                    after_task_resolution === null ? null : await after_task_resolution.id,
                expectedBoardId,
            }),
        )
            .then((new_id: number) => {
                if (generation !== board.board_generation) return `task_${new_id}`;
                // The card may have been edited while its creation was still
                // queued, so update the current card rather than only the
                // original optimistic object.
                const position = this.find_task_position(pending_task_id);
                if (position) {
                    set_task_id(this.columns[position.column_idx].tasks[position.task_idx], new_id);
                }
                this.columns = [...this.columns];
                this.update_labels();
                this.show_success_with_undo("Task added");
                const persisted_task_id = `task_${new_id}`;
                this.resolved_task_ids.set(pending_task_id, persisted_task_id);
                this.rekey_task_move_timer(pending_task_id, persisted_task_id);
                return persisted_task_id;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation !== board.board_generation) throw e;
                const position = this.find_task_position(pending_task_id);
                if (position) {
                    this.columns[position.column_idx].tasks.splice(position.task_idx, 1);
                    this.columns = [...this.columns];
                }
                this.refresh_labels_from_columns();
                toast.error("Couldn't add task");
                throw e;
            });
        this.pending_task_creations.set(pending_task_id, creation);

        return creation
            .then(() => true)
            .catch(() => false)
            .finally(() => this.pending_task_creations.delete(pending_task_id));
    }

    duplicate_task(task_id: string): Promise<boolean> {
        const position = this.find_task_position(task_id);
        if (!position) {
            toast.error("Task no longer exists");
            return Promise.resolve(false);
        }

        const column = this.columns[position.column_idx];
        return this.add_new_task(
            column.id,
            create_task_duplicate(column.tasks[position.task_idx]),
            task_id,
        );
    }

    delete_task(task_id: string): Promise<boolean> {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        const position = this.find_task_position(task_id);
        if (!position) return Promise.resolve(false);
        const task_id_resolution = this.capture_task_id(task_id);
        const current_task_id = this.resolve_task_alias(task_id);
        const deleted_task = clone_task(this.columns[position.column_idx].tasks[position.task_idx]);
        const column_id = this.columns[position.column_idx].id;
        this.cancel_task_move_timer(current_task_id);
        this.columns[position.column_idx].tasks.splice(position.task_idx, 1);
        this.columns = [...this.columns];
        this.refresh_labels_from_columns();

        let task_was_created = !task_id_resolution.was_pending;
        return addMission(async () => {
            const persisted_task_id = await task_id_resolution.id;
            task_was_created = true;
            set_task_id(deleted_task, persisted_task_id);
            await invoke<void>("delete_task", { taskId: persisted_task_id, expectedBoardId });
        })
            .then(() => {
                if (generation !== this.board_generation) return false;
                this.show_success_with_undo("Task deleted");
                return true;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation !== this.board_generation) return false;
                if (task_was_created) {
                    const column = this.columns.find((candidate) => candidate.id === column_id);
                    if (column && !column.tasks.some((task) => task.id === deleted_task.id)) {
                        column.tasks.splice(position.task_idx, 0, deleted_task);
                        this.columns = [...this.columns];
                        this.refresh_labels_from_columns();
                    }
                    toast.error("Couldn't delete task");
                }
                return false;
            });
    }

    archive_task(task_id: string): Promise<boolean> {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        const position = this.find_task_position(task_id);
        if (!position) return Promise.resolve(false);
        const task_id_resolution = this.capture_task_id(task_id);
        const current_task_id = this.resolve_task_alias(task_id);
        const archived_task = clone_task(this.columns[position.column_idx].tasks[position.task_idx]);
        const column_id = this.columns[position.column_idx].id;
        this.cancel_task_move_timer(current_task_id);
        this.columns[position.column_idx].tasks.splice(position.task_idx, 1);
        this.columns = [...this.columns];
        this.refresh_labels_from_columns();

        let task_was_created = !task_id_resolution.was_pending;
        return addMission(async () => {
            const persisted_task_id = await task_id_resolution.id;
            task_was_created = true;
            set_task_id(archived_task, persisted_task_id);
            await invoke<void>("archive_task", { taskId: persisted_task_id, expectedBoardId });
        })
            .then(() => {
                if (generation !== board.board_generation) return false;
                board.get_columns();
                board.refresh_archives_if_loaded();
                this.show_success_with_undo("Task archived");
                return true;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation !== board.board_generation) return false;
                if (task_was_created) {
                    const column = this.columns.find((candidate) => candidate.id === column_id);
                    if (column && !column.tasks.some((task) => task.id === archived_task.id)) {
                        column.tasks.splice(position.task_idx, 0, archived_task);
                        this.columns = [...this.columns];
                        this.refresh_labels_from_columns();
                    }
                    toast.error("Couldn't archive task");
                }
                return false;
            });
    }

    archive_all_tasks(column_id: string) {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        const column = board.columns.find((candidate) => candidate.id === column_id);
        if (!column) return;
        const archived_tasks = column.tasks.map(clone_task);
        column.tasks = [];
        board.columns = [...board.columns];
        board.refresh_labels_from_columns();

        addMission(async () => {
            return invoke<void>("archive_all_tasks", {
                columnId: get_column_id(column_id),
                expectedBoardId,
            })
                .then(() => {
                    if (generation !== board.board_generation) return;
                    board.get_columns();
                    board.refresh_archives_if_loaded();
                    this.show_success_with_undo("Tasks archived");
                })
                .catch((e: string) => {
                    console.log(e);
                    if (generation !== board.board_generation) return;
                    const current_column = board.columns.find(
                        (candidate) => candidate.id === column_id,
                    );
                    if (current_column) {
                        current_column.tasks = archived_tasks;
                        board.columns = [...board.columns];
                        board.refresh_labels_from_columns();
                    }
                    toast.error("Couldn't archive tasks");
                });
        });
    }

    unarchive_task(column_id: string, task_id: string) {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        const archive_task_data = board.archives.find((archive) => archive.task.id === task_id);
        if (!archive_task_data) {
            return;
        }
        const archive_index = board.archives.indexOf(archive_task_data);
        const restored_task = clone_task(archive_task_data.task);
        const column = board.columns.find((candidate) => candidate.id === column_id);
        if (!column) return;
        column.tasks.push(restored_task);
        board.sort_column_tasks(column);
        board.archives.splice(archive_index, 1);
        board.columns = [...board.columns];
        board.archives = [...board.archives];
        board.refresh_labels_from_columns();

        addMission(async () => {
            return invoke<void>("unarchive_task", {
                columnId: get_column_id(column_id),
                taskId: get_task_id(task_id),
                expectedBoardId,
            })
                .then(() => {
                    if (generation !== board.board_generation) return;
                    board.get_archives();
                    this.show_success_with_undo("Task restored");
                })
                .catch((e: string) => {
                    console.log(e);
                    if (generation !== board.board_generation) return;
                    const current_column = board.columns.find(
                        (candidate) => candidate.id === column_id,
                    );
                    if (current_column) {
                        const task_index = current_column.tasks.findIndex(
                            (task) => task.id === task_id,
                        );
                        if (task_index !== -1) current_column.tasks.splice(task_index, 1);
                    }
                    if (!board.archives.some((archive) => archive.task.id === task_id)) {
                        board.archives.splice(archive_index, 0, archive_task_data);
                    }
                    board.columns = [...board.columns];
                    board.archives = [...board.archives];
                    board.refresh_labels_from_columns();
                    toast.error("Couldn't restore task");
                });
        });
    }

    update_task(task_id: string, task: Task): Promise<boolean> {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        const task_id_resolution = this.capture_task_id(task_id);
        const position = this.find_task_position(task_id);
        const previous_task = position
            ? clone_task(this.columns[position.column_idx].tasks[position.task_idx])
            : undefined;
        const updated_task = clone_task(task);
        const resolved_task_id = this.resolve_task_alias(task_id);
        if (resolved_task_id !== task_id) updated_task.id = resolved_task_id;
        if (position) {
            this.columns[position.column_idx].tasks[position.task_idx] = updated_task;
            this.sort_column_tasks(this.columns[position.column_idx]);
            this.columns = [...this.columns];
            board.refresh_labels_from_columns();
        }

        let task_was_created = !task_id_resolution.was_pending;
        let current_task_id = this.resolve_task_alias(task_id);
        return addMission(async () => {
            const persisted_task_id = await task_id_resolution.id;
            task_was_created = true;
            current_task_id = `task_${persisted_task_id}`;
            set_task_id(updated_task, persisted_task_id);
            if (previous_task) set_task_id(previous_task, persisted_task_id);
            await invoke<void>("update_task", {
                taskId: persisted_task_id,
                task: serialize_task(updated_task),
                expectedBoardId,
            });
        })
            .then(() => {
                if (generation !== board.board_generation) return false;
                this.update_labels();
                this.show_success_with_undo("Task updated");
                return true;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation !== board.board_generation) return false;
                const current_position = this.find_task_position(current_task_id);
                if (task_was_created && current_position && previous_task) {
                    this.columns[current_position.column_idx].tasks[current_position.task_idx] = previous_task;
                    this.sort_column_tasks(this.columns[current_position.column_idx]);
                    this.columns = [...this.columns];
                    board.refresh_labels_from_columns();
                }
                if (task_was_created) toast.error("Couldn't update task");
                return false;
            });
    }

    move_task(
        from_column_idx: number,
        from_task_idx: number,
        to_column_idx: number,
        to_task_idx: number | null,
        persist: boolean = true,
    ) {
        if (from_column_idx === to_column_idx && from_task_idx === to_task_idx) {
            return false;
        }

        const from_column = this.columns[from_column_idx];
        const to_column = this.columns[to_column_idx];
        const data = from_column?.tasks[from_task_idx];
        if (!from_column || !to_column || !data) {
            console.warn("Ignoring move with an invalid task position", {
                from_column_idx,
                from_task_idx,
                to_column_idx,
                to_task_idx,
            });
            return false;
        }

        from_column.tasks.splice(from_task_idx, 1);
        if (to_task_idx == null) {
            to_task_idx = to_column.tasks.length;
            to_column.tasks.push(data);
        } else {
            to_column.tasks.splice(to_task_idx, 0, data);
        }
        this.sort_column_tasks(from_column);
        if (to_column !== from_column) this.sort_column_tasks(to_column);
        // Trigger reactivity for Svelte 5
        this.columns = [...this.columns];

        if (!persist) {
            return true;
        }

        const before_task_id = to_column.tasks[to_task_idx + 1]?.id ?? null;
        this.persist_task_move(data.id, to_column.id, before_task_id);
        return true;
    }

    private async show_recovery_messages() {
        try {
            const messages = await invoke<string[]>("take_recovery_messages");
            if (messages.length > 0) {
                toast.warning("Application data recovery", {
                    description: messages.join(" "),
                    duration: 15000,
                });
            }
        } catch (e) {
            console.log("Couldn't load recovery messages:", e);
        }
    }

    persist_task_move(
        task_id: string,
        to_column_id: string,
        before_task_id: string | null,
    ) {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        const task_id_resolution = this.capture_task_id(task_id);
        const before_task_id_resolution =
            before_task_id === null ? null : this.capture_task_id(before_task_id);
        const timer_task_id = this.resolve_task_alias(task_id);
        const existing_timer = this.task_move_timers.get(timer_task_id);
        if (existing_timer) clearTimeout(existing_timer);

        const timer = setTimeout(() => {
            this.forget_task_move_timer(timer);
            addMission(async () => {
                const persisted_task_id = await task_id_resolution.id.catch(() => null);
                if (persisted_task_id === null) return false;
                const persisted_before_task_id =
                    before_task_id_resolution === null
                        ? null
                        : await before_task_id_resolution.id.catch(() => null);
                await invoke<void>("move_task", {
                    taskId: persisted_task_id,
                    toColumnId: get_column_id(to_column_id),
                    beforeTaskId: persisted_before_task_id,
                    expectedBoardId,
                });
                return true;
            })
                .then((persisted) => {
                    if (persisted && generation === this.board_generation) this.can_undo = true;
                })
                .catch((e: unknown) => {
                    console.log(e);
                    if (generation !== this.board_generation) return;
                    toast.error("Couldn't move task");
                    this.get_columns();
                });
        }, DRAG_PERSIST_DEBOUNCE_MS);

        this.task_move_timers.set(timer_task_id, timer);
    }

    // ============ Column Operations ============

    add_column(name: string) {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        addMission(async () => {
            return invoke<number>("add_column", {
                name: name,
                color: "",
                expectedBoardId,
            })
                .then((new_id) => {
                    if (generation !== board.board_generation) return;
                    board.show_success_with_undo("Column added");
                    const column: Column = {
                        id: "",
                        name: name,
                        color: "",
                        sort_order: "custom",
                        tasks: [],
                    };
                    set_column_id(column, new_id);
                    board.columns.push(column);
                    // Trigger reactivity for Svelte 5
                    board.columns = [...board.columns];
                })
                .catch((e: string) => {
                    console.log(e);
                    if (generation !== board.board_generation) return;
                    toast.error("Couldn't add column");
                });
        });
    }

    update_column(column_id: string, name: string, sort_order?: ColumnSort) {
        const board = this;
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        const column = board.columns.find((candidate) => candidate.id === column_id);
        const next_sort_order = sort_order ?? column?.sort_order ?? "custom";
        addMission(async () => {
            return invoke<void>("update_column", {
                columnId: get_column_id(column_id),
                name: name,
                color: "",
                sortOrder: next_sort_order,
                expectedBoardId,
            })
                .then(() => {
                    if (generation !== board.board_generation) return;
                    board.show_success_with_undo("Column updated");
                    const updated_column = board.columns.find((candidate) => candidate.id === column_id);
                    if (updated_column) {
                        updated_column.name = name;
                        updated_column.sort_order = next_sort_order;
                        board.sort_column_tasks(updated_column);
                        board.columns = [...board.columns];
                    }
                })
                .catch((e: string) => {
                    console.log(e);
                    if (generation !== board.board_generation) return;
                    toast.error("Couldn't update column");
                });
        });
    }

    move_column(from_column_idx: number, to_column_idx: number, persist: boolean = true) {
        if (from_column_idx === to_column_idx) {
            return false;
        }

        const data = this.columns[from_column_idx];
        if (!data || !this.columns[to_column_idx]) {
            console.warn("Ignoring move with an invalid column position", {
                from_column_idx,
                to_column_idx,
            });
            return false;
        }

        this.columns.splice(from_column_idx, 1);
        this.columns.splice(to_column_idx, 0, data);
        // Trigger reactivity for Svelte 5
        this.columns = [...this.columns];

        if (!persist) {
            return true;
        }

        const before_column_id = this.columns[to_column_idx + 1]?.id ?? null;
        this.persist_column_move(data.id, before_column_id);
        return true;
    }

    persist_column_move(column_id: string, before_column_id: string | null) {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        const existing_timer = this.column_move_timers.get(column_id);
        if (existing_timer) clearTimeout(existing_timer);

        const timer = setTimeout(() => {
            this.column_move_timers.delete(column_id);
            addMission(async () => {
                return invoke<void>("move_column", {
                    columnId: get_column_id(column_id),
                    beforeColumnId:
                        before_column_id == null ? null : get_column_id(before_column_id),
                    expectedBoardId,
                })
                    .then(() => {
                        if (generation === this.board_generation) this.can_undo = true;
                    })
                    .catch((e: string) => {
                        console.log(e);
                        if (generation !== this.board_generation) return;
                        toast.error("Couldn't move column");
                        this.get_columns();
                    });
            });
        }, DRAG_PERSIST_DEBOUNCE_MS);

        this.column_move_timers.set(column_id, timer);
    }

    delete_column(column_id: string) {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return;
        addMission(async () => {
            return invoke<void>("delete_column", {
                columnId: get_column_id(column_id),
                expectedBoardId,
            })
                .then(() => {
                    if (generation !== this.board_generation) return;
                    const index = this.columns.findIndex((column) => column.id === column_id);
                    if (index !== -1) this.columns.splice(index, 1);
                    this.columns = [...this.columns];
                    this.show_success_with_undo("Column deleted");
                })
                .catch((e: string) => {
                    console.log(e);
                    if (generation !== this.board_generation) return;
                    toast.error("Couldn't delete column");
                });
        });
    }

    // ============ Import/Export ============

    private timestamped_filename(prefix: string): string {
        const now = new Date();
        const part = (value: number) => String(value).padStart(2, "0");
        return `${prefix}-${now.getFullYear()}-${part(now.getMonth() + 1)}-${part(now.getDate())}_${part(now.getHours())}-${part(now.getMinutes())}-${part(now.getSeconds())}.json`;
    }

    async export_to_file() {
        try {
            const file_path = await save({
                filters: [{ name: "JSON", extensions: ["json"] }],
                defaultPath: this.timestamped_filename("cardbe-board-export"),
            });

            if (!file_path) return;

            const expectedBoardId = this.active_board_id;
            if (expectedBoardId === null) return;
            const data = await invoke<string>("export_data", { expectedBoardId });
            await writeTextFile(file_path, data);
            toast.success("Board exported");
        } catch (e) {
            console.log(e);
            toast.error("Couldn't export data");
        }
    }

    async export_all_boards_to_file() {
        try {
            const file_path = await save({ filters: [{ name: "Cardbe everything backup", extensions: ["json"] }], defaultPath: this.timestamped_filename("cardbe-everything-backup") });
            if (!file_path) return;
            await writeTextFile(file_path, await invoke<string>("export_all_boards"));
            toast.success("Everything backed up");
        } catch (e) { console.log(e); toast.error("Couldn't back up everything"); }
    }

    async import_all_boards_from_file(
        confirm_restore = true,
        before_restore?: () => Promise<void>,
    ): Promise<boolean> {
        let share_links_revoked = false;
        try {
            const file_path = await open({ filters: [{ name: "JSON", extensions: ["json"] }], multiple: false });
            if (!file_path) return false;
            const jsonData = await readTextFile(file_path as string);
            const parsed = JSON.parse(jsonData) as { schema_version?: number; boards?: unknown[] };
            if (parsed.schema_version !== 1 || !Array.isArray(parsed.boards) || parsed.boards.length === 0) {
                throw new Error("This is not a valid Cardbe everything backup");
            }
            if (confirm_restore && !window.confirm("Restore everything from this backup? This replaces every board and all notes. Existing LAN share links will be permanently revoked and are not restored.")) return false;
            await before_restore?.();
            share_links_revoked = before_restore !== undefined;
            await invoke("import_all_boards", { jsonData });
            await this.get_boards();
            this.board_generation++; this.column_fetch_finish = false; this.archives_loaded = false;
            this.get_columns(); this.update_labels(); this.get_task_templates(); this.get_expired_tasks(); await this.load_settings();
            toast.success("Everything restored"); return true;
        } catch (e) {
            console.log(e);
            toast.error(share_links_revoked
                ? "Restore failed. Existing LAN share links were already revoked; your boards were not replaced."
                : "Couldn't restore everything");
            return false;
        }
    }

    async prepare_import_from_file(): Promise<ImportCandidate | null> {
        try {
            const file_path = await open({
                filters: [{ name: "JSON", extensions: ["json"] }],
                multiple: false,
            });

            if (!file_path) return null;

            const data = await readTextFile(file_path as string);
            const filename = String(file_path).split(/[\\/]/).pop() ?? "Imported board";
            const board_name = filename.replace(/\.json$/i, "").trim() || "Imported board";
            return {
                json_data: data,
                summary: parse_import_summary(data),
                board_name,
            };
        } catch (e) {
            console.log(e);
            toast.error("Couldn't read import file");
            return null;
        }
    }

    async import_board_as_new(candidate: ImportCandidate): Promise<boolean> {
        try {
            const created = await invoke<BoardSummary>("import_board_as_new", {
                jsonData: candidate.json_data,
                name: candidate.board_name,
            });
            await this.get_boards();
            this.active_board_id = created.id;
            this.board_generation++;
            this.column_fetch_finish = false;
            this.archives_loaded = false;
            this.get_columns(); this.update_labels(); this.get_task_templates(); this.get_expired_tasks();
            toast.success(`Imported as “${created.name}”`);
            return true;
        } catch (e) {
            console.log(e); toast.error("Couldn't import board"); return false;
        }
    }

    import_data(json_data: string): Promise<boolean> {
        const expectedBoardId = this.active_board_id;
        const generation = this.board_generation;
        if (expectedBoardId === null) return Promise.resolve(false);
        return addMission(() => invoke<string>("import_data", { jsonData: json_data, expectedBoardId }))
            .then((snapshot_path) => {
                if (generation !== this.board_generation) return false;
                this.update_labels();
                this.get_columns();
                if (this.archives_loaded) this.get_archives();
                this.get_task_templates();
                this.get_expired_tasks();
                this.can_undo = true;
                toast.success("Data imported", {
                    id: "data-import-success",
                    description: `Pre-import backup: ${snapshot_path}`,
                    duration: 10000,
                    action: {
                        label: "Undo",
                        onClick: () => void this.undo(),
                    },
                });
                return true;
            })
            .catch((e: unknown) => {
                console.log(e);
                if (generation === this.board_generation) toast.error("Couldn't import data");
                return false;
            });
    }
}

// Singleton instance
export const board = new BoardStore();
