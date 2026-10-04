import { m } from "$lib/paraglide/messages.js";
import { applyLanguagePreference, type LanguagePreference } from "$lib/i18n";
import type { TaskExplorerQuery } from "./utils/task-explorer";
import { logger } from "$lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import { writeTextFile, readTextFile } from "@tauri-apps/plugin-fs";
import { toast } from "svelte-sonner";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
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
  type TaskSummary,
  clone_task,
  duplicate_task as create_task_duplicate,
  get_task_id,
  set_task_id,
  serialize_task,
  deserialize_task,
  deserialize_task_summary,
  type TaskSerialized,
  type TaskSummarySerialized,
  type TaskTemplate,
  type TaskTemplateSerialized,
} from "./type/task.svelte";
import type { Archive, ArchiveSummary } from "./type/archive.svelte";
import { parse_import_summary, type ImportSummary } from "./utils/import-data";
import { addMission } from "./utils/mission.svelte";

// Keep drag-and-drop responsive by waiting until the user has finished a
// short sequence of moves before persisting it to disk.
const DRAG_PERSIST_DEBOUNCE_MS = 300;
const COLUMN_FETCH_TIMEOUT_MS = 15_000;
const IROH_BACKGROUND_SYNC_MS = 30_000;
const IROH_VIEWER_SYNC_MS = 180_000;

function with_timeout<T>(
  request: Promise<T>,
  timeout_ms: number,
  message: string,
): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  return new Promise<T>((resolve, reject) => {
    timeout = setTimeout(() => reject(new Error(message)), timeout_ms);
    request.then(resolve, reject).finally(() => {
      if (timeout !== undefined) {
        clearTimeout(timeout);
      }
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

export type BoardRole = "owner" | "editor" | "viewer";
export interface BoardSummary {
  id: number;
  name: string;
  task_count: number;
  shared_role: BoardRole;
  is_shared?: boolean;
  sync_status: string;
  sync_revision: number;
}
interface BoardsState {
  boards: BoardSummary[];
  active_board_id: number;
}
interface IrohSyncResult {
  board: BoardSummary;
  content_changed: boolean;
}

interface ArchivePageSerialized {
  items: Array<{ time: number; task: TaskSummarySerialized }>;
  next_cursor: string | null;
}

interface ExpiredTaskPageSerialized {
  items: TaskSummarySerialized[];
  next_cursor: string | null;
}

interface AllTaskPageSerialized {
  items: Array<{
    board_id: number;
    board_name: string;
    column_name: string | null;
    archived_at: number | null;
    task: TaskSummarySerialized;
  }>;
  next_cursor: string | null;
}

export interface AllTaskItem {
  board_id: number;
  board_name: string;
  column_name: string | undefined;
  archived_at: Date | undefined;
  task: TaskSummary;
}

// ============ Board Store (class-based for Svelte 5 rune compatibility) ============

export class BoardStore {
  // Reactive state
  columns = $state<Column[]>([]);
  labels = $state<string[]>([]);
  archives = $state<Archive[]>([]);
  archive_items = $state<ArchiveSummary[]>([]);
  archive_items_loading = $state(false);
  archive_items_has_more = $state(false);
  archives_loaded = $state(false);
  archives_loading = $state(false);
  expired_tasks = $state<TaskSummary[]>([]);
  expired_tasks_loading = $state(false);
  expired_tasks_has_more = $state(false);
  all_task_items = $state<AllTaskItem[]>([]);
  all_task_items_loading = $state(false);
  all_task_items_error = $state(false);
  all_task_items_has_more = $state(false);
  templates = $state<TaskTemplate[]>([]);
  column_fetch_finish = $state(false);
  column_fetch_error = $state(false);
  notify_enabled = $state(false);
  notification_setting_updating = $state(false);
  private has_undo_history = $state(false);
  get can_edit() {
    const active = this.boards.find(
      (board) => board.id === this.active_board_id,
    );
    return Boolean(
      active &&
      (active.shared_role === "owner" || active.shared_role === "editor"),
    );
  }
  get can_undo() {
    return this.has_undo_history && this.can_edit;
  }
  set can_undo(value: boolean) {
    this.has_undo_history = value && this.can_edit;
  }
  undo_in_progress = $state(false);
  search_task_ids = $state<Set<string> | null>(null);
  search_pending = $state(false);
  search_error = $state(false);
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
  private archive_list_request = 0;
  private archive_list_cursor: string | null = null;
  private archive_list_query = "";
  private archive_list_initialized = false;
  private expired_list_request = 0;
  private expired_list_cursor: string | null = null;
  private expired_list_query = "";
  private expired_list_initialized = false;
  private all_task_list_cursor: string | null = null;
  private all_task_list_filter: TaskExplorerQuery | null = null;
  private all_task_list_request = 0;
  private search_query = "";
  private search_request = 0;
  private search_timer: ReturnType<typeof setTimeout> | undefined;
  private iroh_sync_timer: number | undefined;
  private disposed = false;
  private readonly handle_online = () => this.trigger_iroh_background_sync();
  private readonly handle_visibility_change = () => {
    if (document.visibilityState === "visible") {
      this.trigger_iroh_background_sync();
    }
  };
  private iroh_syncing = new Set<number>();
  private iroh_failures = new Map<number, number>();
  private iroh_retry_after = new Map<number, number>();
  private iroh_last_checked_at = new Map<number, number>();
  private iroh_host_checking = false;
  private iroh_network_generation = 0;
  iroh_last_error = $state("");
  iroh_sync_error = $state<Record<number, string>>({});
  iroh_access_removed = $state<Record<number, boolean>>({});
  iroh_access_error = $state<Record<number, string>>({});
  iroh_last_synced_at = $state<Record<number, number>>({});

  async create_iroh_invite(
    board_id: number,
    permission: "viewer" | "editor",
  ): Promise<string> {
    const ticket = await invoke<string>("create_iroh_invite", {
      boardId: board_id,
      permission,
    });
    await this.get_boards();
    return ticket;
  }

  async join_iroh_invite(ticket: string, silent = false): Promise<boolean> {
    this.iroh_last_error = "";
    try {
      const joined = await invoke<BoardSummary>("join_iroh_invite", {
        ticket,
        requestApproval: !silent,
      });
      this.boards = [...this.boards, joined];
      this.iroh_last_synced_at = {
        ...this.iroh_last_synced_at,
        [joined.id]: Date.now(),
      };
      await this.switch_board(joined.id);
      return true;
    } catch (error) {
      logger.error("iroh.invite_join.failed", error);
      this.iroh_last_error =
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : m.share_join_error();
      if (!silent && !this.iroh_last_error.startsWith("APPROVAL_REQUIRED:")) {
        toast.error(
          this.iroh_last_error.startsWith("Access was declined or revoked")
            ? m.ui_access_was_declined_or_revoked()
            : m.share_join_error(),
        );
      }
      return false;
    }
  }

  async sync_iroh_board(
    board_id = this.active_board_id,
    silent = false,
    access_only = false,
  ): Promise<boolean> {
    if (board_id === null) {
      return false;
    }
    if (this.iroh_syncing.has(board_id)) {
      return false;
    }
    const previous_status = this.boards.find(
      (item) => item.id === board_id,
    )?.sync_status;
    const network_generation = this.iroh_network_generation;
    this.iroh_syncing.add(board_id);
    if (!access_only) {
      this.update_iroh_summary(board_id, "syncing");
    }
    try {
      if (access_only) {
        const [approved, , refreshed] = await invoke<
          [boolean, string, BoardSummary?]
        >("request_iroh_board_access", {
          boardId: board_id,
          requestApproval: false,
        });
        if (!approved) {
          throw "APPROVAL_REQUIRED:Waiting for owner approval.";
        }
        if (refreshed) {
          this.apply_iroh_conflict_refresh(refreshed);
        }
        this.iroh_failures.delete(board_id);
        this.iroh_retry_after.delete(board_id);
        delete this.iroh_sync_error[board_id];
        return true;
      }
      const { board: synced, content_changed } = await invoke<IrohSyncResult>(
        "sync_iroh_board",
        {
          boardId: board_id,
        },
      );
      this.iroh_failures.delete(board_id);
      this.iroh_retry_after.delete(board_id);
      delete this.iroh_sync_error[board_id];
      this.iroh_access_removed = {
        ...this.iroh_access_removed,
        [board_id]: false,
      };
      this.boards = this.boards.map((board) =>
        board.id === synced.id ? synced : board,
      );
      this.iroh_last_synced_at = {
        ...this.iroh_last_synced_at,
        [synced.id]: Date.now(),
      };
      if (board_id === this.active_board_id && content_changed) {
        this.can_undo = false;
        this.reload_active_board_data();
      }
      if (content_changed && this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      if (!silent) {
        toast.success(m.share_synced());
      }
      return true;
    } catch (error) {
      if (network_generation !== this.iroh_network_generation) {
        if (!access_only && previous_status) {
          this.update_iroh_summary(board_id, previous_status);
        }
        return false;
      }
      logger.error("iroh.board_sync.failed", error);
      const raw_message =
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : m.share_sync_error();
      const access_error =
        /^(ACCESS_REVOKED|INVITATION_DISABLED|INVITATION_DELETED|APPROVAL_REQUIRED):/.test(
          raw_message,
        );
      const message = raw_message.replace(
        /^(ACCESS_REVOKED|INVITATION_DISABLED|INVITATION_DELETED|APPROVAL_REQUIRED):/,
        "",
      );
      this.iroh_sync_error[board_id] = message;
      if (
        access_error ||
        message.startsWith("Access was declined or revoked")
      ) {
        this.iroh_access_error[board_id] = message;
        this.iroh_access_removed = {
          ...this.iroh_access_removed,
          [board_id]: true,
        };
      }
      const failures = Math.min(5, (this.iroh_failures.get(board_id) ?? 0) + 1);
      this.iroh_failures.set(board_id, failures);
      this.iroh_retry_after.set(
        board_id,
        Date.now() + Math.min(300_000, IROH_BACKGROUND_SYNC_MS * 2 ** failures),
      );
      await this.get_boards();
      if (
        this.boards.find((board) => board.id === board_id)?.sync_status !==
        "conflict"
      ) {
        this.update_iroh_summary(
          board_id,
          previous_status === "pending" && !access_error ? "pending" : "error",
        );
      }
      if (!silent) {
        toast.error(
          message.startsWith("Access was declined or revoked")
            ? m.ui_access_was_declined_or_revoked()
            : m.share_sync_error(),
        );
      }
      return false;
    } finally {
      // Access-only checks must also yield their place in the sync queue.
      this.iroh_last_checked_at.set(board_id, Date.now());
      this.iroh_syncing.delete(board_id);
      if (network_generation !== this.iroh_network_generation) {
        this.trigger_iroh_background_sync(true);
      }
    }
  }

  async restore_iroh_access(board_id: number) {
    this.iroh_access_removed = {
      ...this.iroh_access_removed,
      [board_id]: false,
    };
    delete this.iroh_access_error[board_id];
    delete this.iroh_sync_error[board_id];
    this.iroh_failures.delete(board_id);
    this.iroh_retry_after.delete(board_id);
    if (
      this.boards.find((item) => item.id === board_id)?.sync_status ===
      "conflict"
    ) {
      toast.success("Access restored. Resolve the conflict before syncing.");
    } else if (await this.sync_iroh_board(board_id, true)) {
      toast.success("Access restored");
    }
  }

  apply_iroh_conflict_refresh(refreshed: BoardSummary) {
    // A delayed access response must not restore an already resolved conflict.
    this.boards = this.boards.map((item) =>
      item.id === refreshed.id && item.sync_status === "conflict"
        ? refreshed
        : item,
    );
  }

  async resolve_iroh_conflict(
    board_id: number,
    keep_local: boolean,
  ): Promise<boolean> {
    try {
      await invoke("resolve_iroh_conflict", {
        boardId: board_id,
        keepLocal: keep_local,
      });
      await this.get_boards();
      this.iroh_failures.delete(board_id);
      this.iroh_retry_after.delete(board_id);
      if (board_id === this.active_board_id && !keep_local) {
        this.can_undo = false;
        this.reload_active_board_data();
      }
      if (this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      toast.success(
        keep_local ? m.share_local_ready() : m.share_owner_restored(),
      );
      if (keep_local) {
        this.trigger_iroh_background_sync();
      }
      return true;
    } catch (error) {
      logger.error("iroh.conflict_resolve.failed", error);
      toast.error(m.share_conflict_error());
      return false;
    }
  }

  /** Called when an owner has accepted an editor's push from another peer. */
  async handle_iroh_remote_push(board_id: number, _revision: number) {
    await this.get_boards();
    if (this.active_board_id === board_id) {
      this.can_undo = false;
      this.reload_active_board_data();
    }
    if (this.all_task_list_filter) {
      void this.fetch_all_task_page(true);
    }
  }

  reconnect_iroh_boards() {
    this.iroh_network_generation += 1;
    this.iroh_failures.clear();
    this.iroh_retry_after.clear();
    this.iroh_sync_error = {};
    this.trigger_iroh_background_sync(true);
  }

  trigger_iroh_background_sync(force = false) {
    if (this.disposed) {
      return;
    }
    if (
      typeof document !== "undefined" &&
      document.visibilityState !== "visible"
    ) {
      return;
    }
    if (!this.iroh_host_checking) {
      this.iroh_host_checking = true;
      void invoke("ensure_iroh_host")
        .catch((error) => {
          logger.warn("iroh.host_start.failed", error);
        })
        .finally(() => {
          this.iroh_host_checking = false;
        });
    }
    for (const item of [...this.boards].sort(
      (a, b) =>
        (this.iroh_last_checked_at.get(a.id) ?? 0) -
        (this.iroh_last_checked_at.get(b.id) ?? 0),
    )) {
      if (
        item.shared_role !== "owner" &&
        !this.iroh_access_removed[item.id] &&
        !this.iroh_syncing.has(item.id) &&
        this.iroh_syncing.size < 2 &&
        Date.now() >= (this.iroh_retry_after.get(item.id) ?? 0)
      ) {
        const access_only =
          item.sync_status === "conflict" ||
          (!force &&
            item.shared_role === "viewer" &&
            Date.now() - (this.iroh_last_synced_at[item.id] ?? 0) <
              IROH_VIEWER_SYNC_MS);
        void this.sync_iroh_board(item.id, true, access_only);
      }
    }
  }

  private update_iroh_summary(board_id: number, sync_status: string) {
    this.boards = this.boards.map((item) =>
      item.id === board_id ? { ...item, sync_status } : item,
    );
  }

  private reload_active_board_data() {
    this.get_columns();
    this.update_labels();
    this.get_task_templates();
    this.get_expired_tasks();
    if (this.archives_loaded) {
      void this.get_archives();
    }
  }

  private start_iroh_background_sync() {
    if (
      this.disposed ||
      this.iroh_sync_timer !== undefined ||
      typeof window === "undefined"
    ) {
      return;
    }
    this.iroh_sync_timer = window.setInterval(
      () => this.trigger_iroh_background_sync(),
      IROH_BACKGROUND_SYNC_MS,
    );
    window.addEventListener("online", this.handle_online);
    document.addEventListener(
      "visibilitychange",
      this.handle_visibility_change,
    );
    this.trigger_iroh_background_sync();
  }

  dispose() {
    this.disposed = true;
    this.stop_expired_task_checker();
    if (this.iroh_sync_timer !== undefined) {
      window.clearInterval(this.iroh_sync_timer);
      this.iroh_sync_timer = undefined;
    }
    if (typeof window !== "undefined") {
      window.removeEventListener("online", this.handle_online);
      document.removeEventListener(
        "visibilitychange",
        this.handle_visibility_change,
      );
    }
    // Let pending user writes finish; only recurring background work is stopped.
  }

  // ============ Data Fetching ============

  private show_data_fetch_error(message: string, retry: () => void) {
    toast.error(message, {
      action: {
        label: m.common_retry(),
        onClick: retry,
      },
    });
  }

  private show_success_with_undo(message: string) {
    this.can_undo = true;
    if (!this.can_undo) {
      toast.success(message);
      return;
    }
    toast.success(message, {
      action: {
        label: m.common_undo(),
        onClick: () => void this.undo(),
      },
    });
  }

  async undo(): Promise<boolean> {
    if (!this.can_undo || this.undo_in_progress) {
      return false;
    }
    this.undo_in_progress = true;
    try {
      const expectedBoardId = this.active_board_id;
      const generation = this.board_generation;
      if (expectedBoardId === null) {
        return false;
      }
      const canUndo = await addMission(() =>
        invoke<boolean>("undo", { expectedBoardId }),
      );
      if (generation !== this.board_generation) {
        return false;
      }
      this.can_undo = canUndo;
      this.get_columns();
      this.refresh_archives_if_loaded();
      this.get_task_templates();
      this.get_expired_tasks();
      this.update_labels();
      toast.success(m.common_undone());
      return true;
    } catch (e: unknown) {
      logger.error("board.undo.failed", e);
      console.log(e);
      toast.error(m.common_undo_error());
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
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(() => invoke<string[]>("get_labels", { expectedBoardId }))
      .then((data) => {
        if (generation !== board.board_generation) {
          return;
        }
        board.labels = data;
      })
      .catch((e) => {
        logger.error("board.labels_load.failed", e);
        console.log(e);
        board.show_data_fetch_error(m.board_labels_load_error(), () =>
          board.update_labels(),
        );
      });
  }

  get_columns() {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    // Only replace the workspace with the loading state during the initial fetch
    // or when retrying an initial-load error.
    // Later refreshes (for example after archiving a recurring task) must keep the
    // existing view mounted so its scroll position is preserved.
    const replaces_workspace =
      !board.column_fetch_finish || board.column_fetch_error;
    if (replaces_workspace) {
      board.column_fetch_finish = false;
      board.column_fetch_error = false;
    }
    // The main workspace readiness gate must not wait behind unrelated
    // background missions (such as loading labels). If one of those IPC
    // calls never settles, queueing this read leaves the app on its
    // initial loading screen forever.
    with_timeout(
      invoke<ColumnSerialized[]>("get_columns", { expectedBoardId }),
      COLUMN_FETCH_TIMEOUT_MS,
      "Loading board data timed out",
    )
      .then((data: ColumnSerialized[]) => {
        if (generation !== board.board_generation) {
          return;
        }
        board.columns = data.map(deserialize_column);
        for (const column of board.columns) {
          board.sort_column_tasks(column);
        }
        board.sync_active_board_task_count();
        board.column_fetch_finish = true;
      })
      .catch((e) => {
        logger.error("board.columns_load.failed", e);
        console.log(e);
        if (replaces_workspace) {
          board.column_fetch_error = true;
          board.column_fetch_finish = true;
        } else {
          board.show_data_fetch_error(m.board_columns_refresh_error(), () =>
            board.get_columns(),
          );
        }
      });
  }

  search_tasks(search_text: string) {
    this.search_query = search_text.trim();
    const request = ++this.search_request;
    if (this.search_timer) {
      clearTimeout(this.search_timer);
    }
    this.search_task_ids = null;
    this.search_error = false;
    this.search_pending = Boolean(this.search_query);
    if (!this.search_query) {
      return;
    }

    const query = this.search_query;
    const query_length = [...query].length;
    const expectedBoardId = this.active_board_id;
    if (expectedBoardId === null) {
      this.search_pending = false;
      return;
    }
    const generation = this.board_generation;
    this.search_timer = setTimeout(async () => {
      if (
        request !== this.search_request ||
        generation !== this.board_generation
      ) {
        return;
      }
      this.search_timer = undefined;
      const started_at = performance.now();
      try {
        const ids = await invoke<number[]>("search_tasks", {
          query,
          expectedBoardId,
        });
        if (
          request !== this.search_request ||
          generation !== this.board_generation
        ) {
          return;
        }
        this.search_task_ids = new Set(ids.map((id) => `task_${id}`));
        logger.debug("board.search.completed", {
          board_id: expectedBoardId,
          query_length,
          result_count: ids.length,
          duration_ms: Math.round(performance.now() - started_at),
        });
      } catch (error) {
        if (
          request !== this.search_request ||
          generation !== this.board_generation
        ) {
          return;
        }
        logger.error("board.search.failed", error, {
          board_id: expectedBoardId,
          query_length,
          duration_ms: Math.round(performance.now() - started_at),
        });
        this.search_error = true;
      } finally {
        if (
          request === this.search_request &&
          generation === this.board_generation
        ) {
          this.search_pending = false;
        }
      }
    }, 160);
  }

  get_archives() {
    if (this.archives_request) {
      return this.archives_request;
    }
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve();
    }
    board.archives_loading = true;
    const request = addMission(() =>
      invoke<{ time: number; task: TaskSerialized }[]>("get_archives", {
        expectedBoardId,
      }),
    )
      .then((data) => {
        if (generation !== board.board_generation) {
          return;
        }
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
        logger.error("board.archives_load.failed", e);
        console.log(e);
        if (generation === board.board_generation) {
          board.show_data_fetch_error(m.task_archives_load_error(), () =>
            board.get_archives(),
          );
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

  open_archive_list() {
    this.archive_list_initialized = true;
    if (this.archive_items.length === 0 && this.archive_list_cursor === null) {
      return this.fetch_archive_page(true);
    }
  }

  search_archives(query: string) {
    this.archive_list_query = query.trim();
    this.fetch_archive_page(true);
  }

  load_more_archives() {
    if (!this.archive_items_loading && this.archive_items_has_more) {
      this.fetch_archive_page(false);
    }
  }

  private fetch_archive_page(reset: boolean) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve();
    }
    const request = ++this.archive_list_request;
    if (reset) {
      this.archive_items = [];
      this.archive_items_loading = false;
      this.archive_list_cursor = null;
      this.archive_items_has_more = true;
    }
    this.archive_items_loading = true;
    return invoke<ArchivePageSerialized>("list_archives", {
      expectedBoardId,
      cursor: reset ? null : this.archive_list_cursor,
      query: this.archive_list_query,
      limit: 50,
    })
      .then((data) => {
        if (
          request !== this.archive_list_request ||
          generation !== this.board_generation
        ) {
          return;
        }
        const items = data.items.map((item) => ({
          time: new Date(item.time),
          task: deserialize_task_summary(item.task),
        }));
        this.archive_items = reset
          ? items
          : [
              ...this.archive_items,
              ...items.filter(
                (item) =>
                  !this.archive_items.some(
                    (existing) => existing.task.id === item.task.id,
                  ),
              ),
            ];
        this.archive_list_cursor = data.next_cursor;
        this.archive_items_has_more = data.next_cursor !== null;
      })
      .catch((error) => {
        logger.error("board.archive_page_load.failed", error);
        if (request === this.archive_list_request) {
          this.show_data_fetch_error(m.task_archives_load_error(), () =>
            this.fetch_archive_page(reset),
          );
        }
      })
      .finally(() => {
        if (request === this.archive_list_request) {
          this.archive_items_loading = false;
        }
      });
  }

  async get_archive_detail(task_id: string): Promise<Task | null> {
    const expectedBoardId = this.active_board_id;
    if (expectedBoardId === null) {
      return null;
    }
    const cached = this.archives.find((archive) => archive.task.id === task_id);
    if (cached) {
      return cached.task;
    }
    try {
      const data = await invoke<TaskSerialized>("get_archive_task", {
        expectedBoardId,
        taskId: get_task_id(task_id),
      });
      return deserialize_task(data);
    } catch (error) {
      logger.error("board.archive_detail_load.failed", error);
      return null;
    }
  }

  ensure_archives_loaded() {
    if (this.archives_loaded) {
      return Promise.resolve();
    }
    return this.get_archives();
  }

  private refresh_archives_if_loaded() {
    if (this.all_task_list_filter) {
      void this.fetch_all_task_page(true);
    }
    if (this.archives_loaded) {
      void this.get_archives();
    }
    if (this.archive_list_initialized) {
      void this.fetch_archive_page(true);
    }
  }

  get_expired_tasks() {
    if (this.expired_list_initialized) {
      this.fetch_expired_page(true);
    }
  }

  open_expired_list() {
    this.expired_list_initialized = true;
    if (this.expired_tasks.length === 0 && this.expired_list_cursor === null) {
      return this.fetch_expired_page(true);
    }
  }

  search_expired_tasks(query: string) {
    this.expired_list_query = query.trim();
    this.fetch_expired_page(true);
  }

  load_more_expired_tasks() {
    if (!this.expired_tasks_loading && this.expired_tasks_has_more) {
      this.fetch_expired_page(false);
    }
  }

  private fetch_expired_page(reset: boolean) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve();
    }
    const request = ++this.expired_list_request;
    if (reset) {
      this.expired_tasks = [];
      this.expired_tasks_loading = false;
      this.expired_list_cursor = null;
      this.expired_tasks_has_more = true;
    }
    this.expired_tasks_loading = true;
    return invoke<ExpiredTaskPageSerialized>("list_expired_tasks", {
      expectedBoardId,
      cursor: reset ? null : this.expired_list_cursor,
      query: this.expired_list_query,
      limit: 50,
    })
      .then((data) => {
        if (
          request !== this.expired_list_request ||
          generation !== this.board_generation
        ) {
          return;
        }
        const items = data.items.map(deserialize_task_summary);
        this.expired_tasks = reset
          ? items
          : [
              ...this.expired_tasks,
              ...items.filter(
                (item) =>
                  !this.expired_tasks.some(
                    (existing) => existing.id === item.id,
                  ),
              ),
            ];
        this.expired_list_cursor = data.next_cursor;
        this.expired_tasks_has_more = data.next_cursor !== null;
      })
      .catch((error) => {
        logger.error("board.expired_tasks_load.failed", error);
        if (request === this.expired_list_request) {
          this.show_data_fetch_error(m.task_expired_load_error(), () =>
            this.fetch_expired_page(reset),
          );
        }
      })
      .finally(() => {
        if (request === this.expired_list_request) {
          this.expired_tasks_loading = false;
        }
      });
  }

  search_all_tasks(filter: TaskExplorerQuery) {
    this.all_task_list_filter = filter;
    return this.fetch_all_task_page(true);
  }

  load_more_all_tasks() {
    if (!this.all_task_items_loading && this.all_task_items_has_more) {
      this.fetch_all_task_page(false);
    }
  }

  private fetch_all_task_page(reset: boolean) {
    if (!this.all_task_list_filter) {
      return Promise.resolve();
    }
    const request = ++this.all_task_list_request;
    if (reset) {
      this.all_task_items = [];
      this.all_task_items_loading = false;
      this.all_task_list_cursor = null;
      this.all_task_items_has_more = true;
    }
    this.all_task_items_loading = true;
    this.all_task_items_error = false;
    return invoke<AllTaskPageSerialized>("list_all_tasks", {
      cursor: reset ? null : this.all_task_list_cursor,
      filter: this.all_task_list_filter,
      limit: 50,
    })
      .then((data) => {
        if (request !== this.all_task_list_request) {
          return;
        }
        const items = data.items.map((item) => ({
          board_id: item.board_id,
          board_name: item.board_name,
          column_name: item.column_name ?? undefined,
          archived_at:
            item.archived_at === null ? undefined : new Date(item.archived_at),
          task: deserialize_task_summary(item.task),
        }));
        this.all_task_items = reset
          ? items
          : [
              ...this.all_task_items,
              ...items.filter(
                (item) =>
                  !this.all_task_items.some(
                    (existing) =>
                      existing.board_id === item.board_id &&
                      existing.archived_at?.getTime() ===
                        item.archived_at?.getTime() &&
                      existing.task.id === item.task.id,
                  ),
              ),
            ];
        this.all_task_list_cursor = data.next_cursor;
        this.all_task_items_has_more = data.next_cursor !== null;
      })
      .catch((error) => {
        logger.error("board.all_task_page_load.failed", error);
        if (request === this.all_task_list_request) {
          this.all_task_items_error = true;
          this.show_data_fetch_error(m.task_explorer_load_error(), () =>
            this.fetch_all_task_page(reset),
          );
        }
      })
      .finally(() => {
        if (request === this.all_task_list_request) {
          this.all_task_items_loading = false;
        }
      });
  }

  async get_expired_task_detail(task_id: string): Promise<Task | null> {
    const expectedBoardId = this.active_board_id;
    if (expectedBoardId === null) {
      return null;
    }
    const active = this.columns
      .flatMap((column) => column.tasks)
      .find((task) => task.id === task_id);
    if (active) {
      return active;
    }
    try {
      const data = await invoke<TaskSerialized>("get_task_detail", {
        expectedBoardId,
        taskId: get_task_id(task_id),
      });
      return deserialize_task(data);
    } catch (error) {
      logger.error("board.expired_task_detail_load.failed", error);
      return null;
    }
  }

  async init() {
    await this.get_boards();
    this.update_labels();
    this.get_columns();
    this.get_task_templates();

    // Load settings
    await this.load_settings();
    await this.show_recovery_messages();
    this.start_iroh_background_sync();

    // Check notification permission and start expired task checker (only if enabled)
    if (this.notify_enabled) {
      const permission_granted = await this.enable_expired_task_notifications();
      if (!permission_granted) {
        await this.set_notify_enabled(false);
        toast.warning(m.settings_notify_permission_disabled());
      }
    }
  }

  private sync_active_board_task_count() {
    const active_board_id = this.active_board_id;
    if (active_board_id === null) {
      return;
    }
    const task_count = this.columns.reduce(
      (total, column) => total + column.tasks.length,
      0,
    );
    this.boards = this.boards.map((item) =>
      item.id === active_board_id && item.task_count !== task_count
        ? { ...item, task_count }
        : item,
    );
  }

  async get_boards() {
    const generation = this.board_generation;
    try {
      const state = await invoke<BoardsState>("get_boards");
      if (generation !== this.board_generation) {
        return;
      }
      this.boards = state.boards;
      this.active_board_id = state.active_board_id;
      if (!this.can_edit) {
        this.can_undo = false;
      }
    } catch (e) {
      logger.error("board.list_load.failed", e);
      console.log("Couldn't load boards:", e);
    }
  }

  async create_board(name: string) {
    const trimmed = name.trim();
    if (!trimmed) {
      return false;
    }
    try {
      const created = await invoke<BoardSummary>("create_board", {
        name: trimmed,
      });
      this.boards = [...this.boards, created];
      await this.switch_board(created.id);
      return true;
    } catch (e) {
      logger.error("board.create.failed", e);
      console.log(e);
      toast.error(m.board_create_error());
      return false;
    }
  }

  async rename_board(id: number, name: string) {
    const trimmed = name.trim();
    if (!trimmed) {
      return false;
    }
    try {
      await invoke("rename_board", { boardId: id, name: trimmed });
      this.boards = this.boards.map((board) =>
        board.id === id ? { ...board, name: trimmed } : board,
      );
      return true;
    } catch (e) {
      logger.error("board.rename.failed", e);
      console.log(e);
      toast.error(m.board_rename_error());
      return false;
    }
  }

  async switch_board(id: number): Promise<boolean> {
    if (id === this.active_board_id) {
      return true;
    }
    const archive_list_was_initialized = this.archive_list_initialized;
    const expired_list_was_initialized = this.expired_list_initialized;
    try {
      this.board_generation++;
      for (const timer of this.task_move_timers.values()) {
        clearTimeout(timer);
      }
      for (const timer of this.column_move_timers.values()) {
        clearTimeout(timer);
      }
      this.task_move_timers.clear();
      this.column_move_timers.clear();
      await invoke("switch_board", { boardId: id });
      this.active_board_id = id;
      this.archives = [];
      this.archive_items = [];
      this.archive_items_loading = false;
      this.archive_items_has_more = false;
      this.archive_list_cursor = null;
      this.archive_list_initialized = archive_list_was_initialized;
      this.archive_list_request++;
      this.archives_loaded = false;
      this.archives_loading = false;
      this.archives_request = undefined;
      this.templates = [];
      this.labels = [];
      this.expired_tasks = [];
      this.expired_tasks_loading = false;
      this.expired_tasks_has_more = false;
      this.expired_list_cursor = null;
      this.expired_list_initialized = expired_list_was_initialized;
      this.expired_list_request++;
      this.can_undo = false;
      this.get_columns();
      this.update_labels();
      this.get_task_templates();
      if (archive_list_was_initialized) {
        void this.fetch_archive_page(true);
      }
      if (expired_list_was_initialized) {
        void this.fetch_expired_page(true);
      }
      return true;
    } catch (e) {
      logger.error("board.switch.failed", e);
      console.log(e);
      toast.error(m.board_switch_error());
      return false;
    }
  }

  async delete_board(id: number) {
    try {
      const next = await invoke<number>("delete_board", { boardId: id });
      this.boards = this.boards.filter((board) => board.id !== id);
      if (id === this.active_board_id) {
        this.active_board_id = null;
        await this.switch_board(next);
      }
      return true;
    } catch (e) {
      logger.error("board.delete.failed", e);
      console.log(e);
      toast.error(m.board_delete_error());
      return false;
    }
  }

  private async load_settings() {
    try {
      const settings = await invoke<{
        notify_enabled: boolean;
        language?: LanguagePreference;
      }>("get_settings");
      this.notify_enabled = settings.notify_enabled;
      applyLanguagePreference(settings.language);
    } catch (e) {
      logger.warn("settings.load.failed", e);
      console.log("Couldn't load settings:", e);
    }
  }

  async set_notify_enabled(enabled: boolean) {
    if (this.notification_setting_updating) {
      return;
    }
    this.notification_setting_updating = true;

    try {
      if (enabled && !(await this.request_notification_permission())) {
        toast.error(m.settings_notify_permission_error());
        return;
      }

      const saved = await addMission(async () => {
        try {
          await invoke<void>("set_notify_enabled", { enabled });
          return true;
        } catch (e: unknown) {
          logger.error("settings.notification_save.failed", e);
          console.log("Couldn't save notification settings:", e);
          return false;
        }
      });
      if (!saved) {
        toast.error(m.settings_notify_save_error());
        return;
      }

      this.notify_enabled = enabled;
      if (enabled) {
        this.start_expired_task_checker();
      } else {
        this.stop_expired_task_checker();
      }
    } finally {
      this.notification_setting_updating = false;
    }
  }

  async set_language(preference: LanguagePreference): Promise<boolean> {
    const saved = await addMission(async () => {
      try {
        await invoke("set_language", { language: preference });
        return true;
      } catch (error) {
        logger.error("settings.language_save.failed", error);
        return false;
      }
    });
    if (saved) {
      applyLanguagePreference(preference);
    }
    return saved;
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
      logger.warn("notifications.permission_request.failed", e);
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
    if (this.disposed || this.expired_task_checker !== undefined) {
      return;
    }

    const check_expired_tasks = async () => {
      if (!this.notify_enabled) {
        return;
      }
      try {
        await invoke("check_expired_tasks", {
          titleTemplate: m.notification_expired_title({ board: "{board}" }),
          bodyTemplate: m.notification_expired_body({ task: "{task}" }),
        });
      } catch (e: unknown) {
        logger.warn("notifications.expired_task_check.failed", e);
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
    if (this.expired_task_checker === undefined) {
      return;
    }
    clearInterval(this.expired_task_checker);
    this.expired_task_checker = undefined;
  }

  // ============ Task Operations ============

  sort_column_tasks(column: Column) {
    const direction = column.sort_order === "due_date_asc" ? 1 : -1;
    column.tasks.sort((a, b) => {
      const pinned_order =
        Number(b.pinned ?? false) - Number(a.pinned ?? false);
      if (pinned_order || column.sort_order === "custom") {
        return pinned_order;
      }
      if (!a.due_time) {
        return b.due_time ? 1 : 0;
      }
      if (!b.due_time) {
        return -1;
      }
      return (a.due_time.getTime() - b.due_time.getTime()) * direction;
    });
  }

  get_task_templates() {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(() =>
      invoke<TaskTemplateSerialized[]>("get_task_templates", {
        expectedBoardId,
      }),
    )
      .then((templates) => {
        if (generation !== this.board_generation) {
          return;
        }
        this.templates = templates.map((template) => {
          const task = deserialize_task(template.task);
          task.id = "";
          return { id: template.id, name: template.name, task };
        });
      })
      .catch((e: unknown) => {
        logger.error("template.load.failed", e);
        console.log(e);
        if (generation === this.board_generation) {
          toast.error(m.task_template_load_error());
        }
      });
  }

  save_task_template(task_id: string, name: string): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    return addMission(async () =>
      invoke<TaskTemplateSerialized>("save_task_template", {
        taskId: await task_id_resolution.id,
        name,
        expectedBoardId,
      }),
    )
      .then((template) => {
        if (generation !== this.board_generation) {
          return false;
        }
        const task = deserialize_task(template.task);
        task.id = "";
        this.templates = [
          ...this.templates,
          { id: template.id, name: template.name, task },
        ];
        this.show_success_with_undo(m.task_template_saved());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("template.create.failed", e);
        console.log(e);
        if (generation === this.board_generation) {
          toast.error(m.task_template_save_error());
        }
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
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    return addMission(() =>
      invoke<TaskTemplateSerialized>("update_task_template", {
        templateId: template_id,
        name,
        task: serialize_task(task),
        expectedBoardId,
      }),
    )
      .then((template) => {
        if (generation !== this.board_generation) {
          return false;
        }
        const updated_task = deserialize_task(template.task);
        updated_task.id = "";
        this.templates = this.templates.map((candidate) =>
          candidate.id === template.id
            ? { id: template.id, name: template.name, task: updated_task }
            : candidate,
        );
        this.show_success_with_undo(m.task_template_updated());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("template.update.failed", e);
        console.log(e);
        if (generation === this.board_generation) {
          toast.error(m.task_template_update_error());
        }
        return false;
      });
  }

  delete_task_template(template_id: number): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    return addMission(() =>
      invoke<void>("delete_task_template", {
        templateId: template_id,
        expectedBoardId,
      }),
    )
      .then(() => {
        if (generation !== this.board_generation) {
          return false;
        }
        this.templates = this.templates.filter(
          (template) => template.id !== template_id,
        );
        this.show_success_with_undo(m.task_template_deleted());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("template.delete.failed", e);
        console.log(e);
        if (generation === this.board_generation) {
          toast.error(m.task_template_delete_error());
        }
        return false;
      });
  }

  private find_task_position(task_id: string) {
    const current_task_id = this.resolve_task_alias(task_id);
    for (let column_idx = 0; column_idx < this.columns.length; column_idx++) {
      const task_idx = this.columns[column_idx].tasks.findIndex(
        (task) => task.id === current_task_id,
      );
      if (task_idx !== -1) {
        return { column_idx, task_idx };
      }
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
      id: (pending_creation ?? Promise.resolve(task_id)).then(
        (persisted_task_id) => get_task_id(persisted_task_id),
      ),
    };
  }

  private cancel_task_move_timer(task_id: string) {
    const timer = this.task_move_timers.get(task_id);
    if (!timer) {
      return;
    }
    clearTimeout(timer);
    this.task_move_timers.delete(task_id);
  }

  private forget_task_move_timer(timer: ReturnType<typeof setTimeout>) {
    for (const [task_id, candidate] of this.task_move_timers) {
      if (candidate === timer) {
        this.task_move_timers.delete(task_id);
      }
    }
  }

  private rekey_task_move_timer(
    pending_task_id: string,
    persisted_task_id: string,
  ) {
    const timer = this.task_move_timers.get(pending_task_id);
    if (!timer) {
      return;
    }
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
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const new_task = clone_task(task);

    // Use a temporary ID so the new card can appear before the backend has
    // allocated its persistent ID.
    new_task.id = `pending_task_${globalThis.crypto.randomUUID()}`;
    const pending_task_id = new_task.id;
    const column = this.columns.find((candidate) => candidate.id === column_id);
    if (!column) {
      toast.error(m.column_missing());
      return Promise.resolve(false);
    }
    const after_task_idx =
      after_task_id === null
        ? -1
        : column.tasks.findIndex(
            (candidate) =>
              candidate.id === this.resolve_task_alias(after_task_id),
          );
    if (after_task_id !== null && after_task_idx === -1) {
      toast.error(m.task_original_missing());
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
          after_task_resolution === null
            ? null
            : await after_task_resolution.id,
        expectedBoardId,
      }),
    )
      .then((new_id: number) => {
        if (generation !== board.board_generation) {
          return `task_${new_id}`;
        }
        // The card may have been edited while its creation was still
        // queued, so update the current card rather than only the
        // original optimistic object.
        const position = this.find_task_position(pending_task_id);
        if (position) {
          set_task_id(
            this.columns[position.column_idx].tasks[position.task_idx],
            new_id,
          );
        }
        this.columns = [...this.columns];
        this.update_labels();
        this.show_success_with_undo(m.task_added());
        const persisted_task_id = `task_${new_id}`;
        this.resolved_task_ids.set(pending_task_id, persisted_task_id);
        this.rekey_task_move_timer(pending_task_id, persisted_task_id);
        return persisted_task_id;
      })
      .catch((e: unknown) => {
        logger.error("task.create.failed", e);
        console.log(e);
        if (generation !== board.board_generation) {
          throw e;
        }
        const position = this.find_task_position(pending_task_id);
        if (position) {
          this.columns[position.column_idx].tasks.splice(position.task_idx, 1);
          this.columns = [...this.columns];
        }
        this.refresh_labels_from_columns();
        toast.error(m.task_add_error());
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
      toast.error(m.task_missing());
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
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const position = this.find_task_position(task_id);
    if (!position) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const current_task_id = this.resolve_task_alias(task_id);
    const deleted_task = clone_task(
      this.columns[position.column_idx].tasks[position.task_idx],
    );
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
      await invoke<void>("delete_task", {
        taskId: persisted_task_id,
        expectedBoardId,
      });
    })
      .then(() => {
        if (generation !== this.board_generation) {
          return false;
        }
        this.search_tasks(this.search_query);
        this.show_success_with_undo(m.task_deleted());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("task.delete.failed", e);
        console.log(e);
        if (generation !== this.board_generation) {
          return false;
        }
        if (task_was_created) {
          const column = this.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (
            column &&
            !column.tasks.some((task) => task.id === deleted_task.id)
          ) {
            column.tasks.splice(position.task_idx, 0, deleted_task);
            this.columns = [...this.columns];
            this.refresh_labels_from_columns();
          }
          toast.error(m.task_delete_error());
        }
        return false;
      });
  }

  archive_task(task_id: string): Promise<boolean> {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const position = this.find_task_position(task_id);
    if (!position) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const current_task_id = this.resolve_task_alias(task_id);
    const archived_task = clone_task(
      this.columns[position.column_idx].tasks[position.task_idx],
    );
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
      await invoke<void>("archive_task", {
        taskId: persisted_task_id,
        expectedBoardId,
      });
    })
      .then(() => {
        if (generation !== board.board_generation) {
          return false;
        }
        board.get_columns();
        board.refresh_archives_if_loaded();
        this.show_success_with_undo(m.task_archived());
        return true;
      })
      .catch((e: unknown) => {
        logger.error("task.archive.failed", e);
        console.log(e);
        if (generation !== board.board_generation) {
          return false;
        }
        if (task_was_created) {
          const column = this.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (
            column &&
            !column.tasks.some((task) => task.id === archived_task.id)
          ) {
            column.tasks.splice(position.task_idx, 0, archived_task);
            this.columns = [...this.columns];
            this.refresh_labels_from_columns();
          }
          toast.error(m.task_archive_error());
        }
        return false;
      });
  }

  archive_all_tasks(column_id: string) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const column = board.columns.find(
      (candidate) => candidate.id === column_id,
    );
    if (!column) {
      return;
    }
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
          if (generation !== board.board_generation) {
            return;
          }
          board.get_columns();
          board.refresh_archives_if_loaded();
          this.show_success_with_undo(m.task_archived_all());
        })
        .catch((e: string) => {
          logger.error("task.archive_all.failed", e);
          console.log(e);
          if (generation !== board.board_generation) {
            return;
          }
          const current_column = board.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (current_column) {
            current_column.tasks = archived_tasks;
            board.columns = [...board.columns];
            board.refresh_labels_from_columns();
          }
          toast.error(m.task_archive_all_error());
        });
    });
  }

  unarchive_task(column_id: string, task_id: string) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const archive_task_data = board.archives.find(
      (archive) => archive.task.id === task_id,
    );
    if (!archive_task_data) {
      return;
    }
    const archive_index = board.archives.indexOf(archive_task_data);
    const restored_task = clone_task(archive_task_data.task);
    const column = board.columns.find(
      (candidate) => candidate.id === column_id,
    );
    if (!column) {
      return;
    }
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
          if (generation !== board.board_generation) {
            return;
          }
          board.get_archives();
          this.show_success_with_undo(m.task_restored());
        })
        .catch((e: string) => {
          logger.error("task.unarchive.failed", e);
          console.log(e);
          if (generation !== board.board_generation) {
            return;
          }
          const current_column = board.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (current_column) {
            const task_index = current_column.tasks.findIndex(
              (task) => task.id === task_id,
            );
            if (task_index !== -1) {
              current_column.tasks.splice(task_index, 1);
            }
          }
          if (!board.archives.some((archive) => archive.task.id === task_id)) {
            board.archives.splice(archive_index, 0, archive_task_data);
          }
          board.columns = [...board.columns];
          board.archives = [...board.archives];
          board.refresh_labels_from_columns();
          toast.error(m.task_restore_error());
        });
    });
  }

  unarchive_task_from_list(column_id: string, task_id: string) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(() =>
      invoke<void>("unarchive_task", {
        columnId: get_column_id(column_id),
        taskId: get_task_id(task_id),
        expectedBoardId,
      }),
    )
      .then(() => {
        if (generation !== this.board_generation) {
          return;
        }
        this.archive_items = this.archive_items.filter(
          (item) => item.task.id !== task_id,
        );
        this.get_columns();
        this.refresh_archives_if_loaded();
        this.show_success_with_undo(m.task_restored());
      })
      .catch((error: unknown) => {
        logger.error("task.unarchive_from_list.failed", error);
        if (generation === this.board_generation) {
          toast.error(m.task_restore_error());
        }
      });
  }

  update_task(task_id: string, task: Task): Promise<boolean> {
    if (!this.can_edit) {
      return Promise.resolve(false);
    }
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const position = this.find_task_position(task_id);
    const previous_task = position
      ? clone_task(this.columns[position.column_idx].tasks[position.task_idx])
      : undefined;
    const updated_task = clone_task(task);
    const resolved_task_id = this.resolve_task_alias(task_id);
    if (resolved_task_id !== task_id) {
      updated_task.id = resolved_task_id;
    }
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
      if (previous_task) {
        set_task_id(previous_task, persisted_task_id);
      }
      await invoke<void>("update_task", {
        taskId: persisted_task_id,
        task: serialize_task(updated_task),
        expectedBoardId,
      });
    })
      .then(() => {
        if (generation !== board.board_generation) {
          return false;
        }
        this.update_labels();
        this.search_tasks(this.search_query);
        this.show_success_with_undo(m.task_updated());
        if (this.all_task_list_filter) {
          void this.fetch_all_task_page(true);
        }
        return true;
      })
      .catch((e: unknown) => {
        logger.error("task.update.failed", e);
        console.log(e);
        if (generation !== board.board_generation) {
          return false;
        }
        const current_position = this.find_task_position(current_task_id);
        if (task_was_created && current_position && previous_task) {
          this.columns[current_position.column_idx].tasks[
            current_position.task_idx
          ] = previous_task;
          this.sort_column_tasks(this.columns[current_position.column_idx]);
          this.columns = [...this.columns];
          board.refresh_labels_from_columns();
        }
        if (task_was_created) {
          toast.error(m.task_update_error());
        }
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
      const context = {
        from_column_idx,
        from_task_idx,
        to_column_idx,
        to_task_idx,
      };
      logger.warn("board.task_move.invalid_position", undefined, context);
      console.warn("Ignoring move with an invalid task position", context);
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
    if (to_column !== from_column) {
      this.sort_column_tasks(to_column);
    }
    // Trigger reactivity for Svelte 5
    this.columns = [...this.columns];

    if (!persist) {
      return true;
    }

    const final_task_idx = to_column.tasks.findIndex(
      (task) => task.id === data.id,
    );
    const before_task_id = to_column.tasks[final_task_idx + 1]?.id ?? null;
    this.persist_task_move(data.id, to_column.id, before_task_id);
    return true;
  }

  private async show_recovery_messages() {
    try {
      const messages = await invoke<string[]>("take_recovery_messages");
      if (messages.length > 0) {
        toast.warning(m.backup_recovery(), {
          description: messages.join(" "),
          duration: 15000,
        });
      }
    } catch (e) {
      logger.warn("storage.recovery_messages_load.failed", e);
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
    if (expectedBoardId === null) {
      return;
    }
    const task_id_resolution = this.capture_task_id(task_id);
    const before_task_id_resolution =
      before_task_id === null ? null : this.capture_task_id(before_task_id);
    const timer_task_id = this.resolve_task_alias(task_id);
    const existing_timer = this.task_move_timers.get(timer_task_id);
    if (existing_timer) {
      clearTimeout(existing_timer);
    }

    const timer = setTimeout(() => {
      this.forget_task_move_timer(timer);
      addMission(async () => {
        const persisted_task_id = await task_id_resolution.id.catch(() => null);
        if (persisted_task_id === null) {
          return false;
        }
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
          if (persisted && generation === this.board_generation) {
            this.can_undo = true;
          }
        })
        .catch((e: unknown) => {
          logger.error("task.move.failed", e);
          console.log(e);
          if (generation !== this.board_generation) {
            return;
          }
          toast.error(m.task_move_error());
          this.get_columns();
        });
    }, DRAG_PERSIST_DEBOUNCE_MS);

    this.task_move_timers.set(timer_task_id, timer);
  }

  async move_task_to_board(
    task_id: string,
    target_board_id: number,
    target_column_id: number,
  ): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    const resolution = this.capture_task_id(task_id);
    this.cancel_task_move_timer(this.resolve_task_alias(task_id));
    try {
      await addMission(async () => {
        await invoke("move_task_to_board", {
          expectedBoardId,
          taskId: await resolution.id,
          targetBoardId: target_board_id,
          targetColumnId: target_column_id,
        });
        // Clear undo before the next queued edit can create new history.
        if (generation === this.board_generation) {
          this.can_undo = false;
        }
      });
      await this.get_boards();
      if (this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      if (generation === this.board_generation) {
        this.get_columns();
        this.update_labels();
        this.search_tasks(this.search_query);
        if (this.expired_list_initialized) {
          void this.fetch_expired_page(true);
        }
      }
      toast.success(m.task_moved_board());
      return true;
    } catch (e) {
      logger.error("task.move_to_board.failed", e);
      if (generation === this.board_generation) {
        this.get_columns();
      }
      toast.error(m.task_move_error());
      return false;
    }
  }

  // ============ Column Operations ============

  add_column(name: string) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(async () => {
      return invoke<number>("add_column", {
        name: name,
        color: "",
        expectedBoardId,
      })
        .then((new_id) => {
          if (generation !== board.board_generation) {
            return;
          }
          board.show_success_with_undo(m.column_added());
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
          logger.error("column.create.failed", e);
          console.log(e);
          if (generation !== board.board_generation) {
            return;
          }
          toast.error(m.column_add_error());
        });
    });
  }

  update_column(column_id: string, name: string, sort_order?: ColumnSort) {
    const board = this;
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    const column = board.columns.find(
      (candidate) => candidate.id === column_id,
    );
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
          if (generation !== board.board_generation) {
            return;
          }
          board.show_success_with_undo(m.column_updated());
          const updated_column = board.columns.find(
            (candidate) => candidate.id === column_id,
          );
          if (updated_column) {
            updated_column.name = name;
            updated_column.sort_order = next_sort_order;
            board.sort_column_tasks(updated_column);
            board.columns = [...board.columns];
          }
        })
        .catch((e: string) => {
          logger.error("column.update.failed", e);
          console.log(e);
          if (generation !== board.board_generation) {
            return;
          }
          toast.error(m.column_update_error());
        });
    });
  }

  move_column(
    from_column_idx: number,
    to_column_idx: number,
    persist: boolean = true,
  ) {
    if (from_column_idx === to_column_idx) {
      return false;
    }

    const data = this.columns[from_column_idx];
    if (!data || !this.columns[to_column_idx]) {
      const context = {
        from_column_idx,
        to_column_idx,
      };
      logger.warn("board.column_move.invalid_position", undefined, context);
      console.warn("Ignoring move with an invalid column position", context);
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
    if (expectedBoardId === null) {
      return;
    }
    const existing_timer = this.column_move_timers.get(column_id);
    if (existing_timer) {
      clearTimeout(existing_timer);
    }

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
            if (generation === this.board_generation) {
              this.can_undo = true;
            }
          })
          .catch((e: string) => {
            logger.error("column.move.failed", e);
            console.log(e);
            if (generation !== this.board_generation) {
              return;
            }
            toast.error(m.column_move_error());
            this.get_columns();
          });
      });
    }, DRAG_PERSIST_DEBOUNCE_MS);

    this.column_move_timers.set(column_id, timer);
  }

  delete_column(column_id: string) {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return;
    }
    addMission(async () => {
      return invoke<void>("delete_column", {
        columnId: get_column_id(column_id),
        expectedBoardId,
      })
        .then(() => {
          if (generation !== this.board_generation) {
            return;
          }
          const index = this.columns.findIndex(
            (column) => column.id === column_id,
          );
          if (index !== -1) {
            this.columns.splice(index, 1);
          }
          this.columns = [...this.columns];
          this.show_success_with_undo(m.column_deleted());
        })
        .catch((e: string) => {
          logger.error("column.delete.failed", e);
          console.log(e);
          if (generation !== this.board_generation) {
            return;
          }
          toast.error(m.column_delete_error());
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

      if (!file_path) {
        return;
      }

      const expectedBoardId = this.active_board_id;
      if (expectedBoardId === null) {
        return;
      }
      const data = await invoke<string>("export_data", { expectedBoardId });
      await writeTextFile(file_path, data);
      toast.success(m.board_exported());
    } catch (e) {
      logger.error("board.export.failed", e);
      console.log(e);
      toast.error(m.board_export_error());
    }
  }

  async export_all_boards_to_file() {
    try {
      const file_path = await save({
        filters: [{ name: m.backup_file_type(), extensions: ["json"] }],
        defaultPath: this.timestamped_filename("cardbe-everything-backup"),
      });
      if (!file_path) {
        return;
      }
      await writeTextFile(file_path, await invoke<string>("export_all_boards"));
      toast.success(m.backup_created());
    } catch (e) {
      logger.error("backup.export.failed", e);
      console.log(e);
      toast.error(m.backup_create_error());
    }
  }

  async import_all_boards_from_file(
    confirm_restore = true,
    before_restore?: () => Promise<void>,
  ): Promise<boolean> {
    let share_links_revoked = false;
    try {
      const file_path = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
      });
      if (!file_path) {
        return false;
      }
      const jsonData = await readTextFile(file_path as string);
      const parsed = JSON.parse(jsonData) as {
        schema_version?: number;
        boards?: unknown[];
      };
      if (
        parsed.schema_version !== 1 ||
        !Array.isArray(parsed.boards) ||
        parsed.boards.length === 0
      ) {
        throw new Error(m.backup_invalid());
      }
      if (confirm_restore && !window.confirm(m.backup_restore_confirm())) {
        return false;
      }
      await before_restore?.();
      share_links_revoked = before_restore !== undefined;
      await invoke("import_all_boards", { jsonData });
      await this.get_boards();
      this.board_generation++;
      this.column_fetch_finish = false;
      this.archives_loaded = false;
      this.get_columns();
      this.update_labels();
      this.get_task_templates();
      this.get_expired_tasks();
      await this.load_settings();
      toast.success(m.backup_restored());
      return true;
    } catch (e) {
      logger.error("backup.restore.failed", e);
      console.log(e);
      toast.error(
        share_links_revoked
          ? m.backup_restore_revoked_error()
          : m.backup_restore_error(),
      );
      return false;
    }
  }

  async prepare_import_from_file(): Promise<ImportCandidate | null> {
    try {
      const file_path = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
      });

      if (!file_path) {
        return null;
      }

      const data = await readTextFile(file_path as string);
      const filename =
        String(file_path).split(/[\\/]/).pop() ?? m.ui_imported_board();
      const board_name =
        filename.replace(/\.json$/i, "").trim() || m.ui_imported_board();
      return {
        json_data: data,
        summary: parse_import_summary(data),
        board_name,
      };
    } catch (e) {
      logger.error("board.import_file_read.failed", e);
      console.log(e);
      toast.error(m.backup_import_read_error());
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
      this.get_columns();
      this.update_labels();
      this.get_task_templates();
      this.get_expired_tasks();
      toast.success(m.board_imported_named({ name: created.name }));
      return true;
    } catch (e) {
      logger.error("board.import_as_new.failed", e);
      console.log(e);
      toast.error(m.board_import_error());
      return false;
    }
  }

  import_data(json_data: string): Promise<boolean> {
    const expectedBoardId = this.active_board_id;
    const generation = this.board_generation;
    if (expectedBoardId === null) {
      return Promise.resolve(false);
    }
    return addMission(() =>
      invoke<string>("import_data", { jsonData: json_data, expectedBoardId }),
    )
      .then((snapshot_path) => {
        if (generation !== this.board_generation) {
          return false;
        }
        this.update_labels();
        this.get_columns();
        if (this.archives_loaded) {
          this.get_archives();
        }
        this.get_task_templates();
        this.get_expired_tasks();
        this.can_undo = true;
        toast.success(m.backup_imported(), {
          id: "data-import-success",
          description: `Pre-import backup: ${snapshot_path}`,
          duration: 10000,
          action: this.can_undo
            ? {
                label: m.common_undo(),
                onClick: () => void this.undo(),
              }
            : undefined,
        });
        return true;
      })
      .catch((e: unknown) => {
        logger.error("board.import_data.failed", e);
        console.log(e);
        if (generation === this.board_generation) {
          toast.error(m.backup_import_error());
        }
        return false;
      });
  }
}

// Singleton instance
export const board = new BoardStore();

if (import.meta.hot) {
  import.meta.hot.dispose(() => board.dispose());
}
