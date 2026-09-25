import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { BoardStore } from "./board.svelte";
import { create_task } from "./type/task.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-fs", () => ({
    readTextFile: vi.fn(),
    writeTextFile: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-notification", () => ({
    isPermissionGranted: vi.fn(),
    requestPermission: vi.fn(),
}));
vi.mock("svelte-sonner", () => ({
    toast: {
        error: vi.fn(),
        success: vi.fn(),
        warning: vi.fn(),
    },
}));

const invoke_mock = vi.mocked(invoke);

function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((resolve_promise, reject_promise) => {
        resolve = resolve_promise;
        reject = reject_promise;
    });
    return { promise, resolve, reject };
}

function create_store(column_count = 1) {
    const store = new BoardStore();
    store.boards = [{ id: 7, name: "Test board", task_count: 0, shared_role: "owner", sync_status: "synced", sync_revision: 0 }];
    store.active_board_id = 7;
    store.columns = Array.from({ length: column_count }, (_, index) => ({
        id: `column_${index + 1}`,
        name: `Column ${index + 1}`,
        color: "",
        sort_order: "custom",
        tasks: [],
    }));
    return store;
}

describe("initial board loading", () => {
    beforeEach(() => {
        invoke_mock.mockReset();
    });

    it("loads columns without waiting for an earlier background mission", async () => {
        const labels = deferred<string[]>();
        invoke_mock.mockImplementation((command) => {
            if (command === "get_labels") return labels.promise;
            if (command === "get_columns") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store();

        store.update_labels();
        store.get_columns();

        expect(invoke_mock).toHaveBeenCalledWith("get_columns", { expectedBoardId: 7 });
        await Promise.resolve();
        await Promise.resolve();
        expect(store.column_fetch_finish).toBe(true);
        expect(store.column_fetch_error).toBe(false);
        labels.resolve([]);
        await Promise.resolve();
    });

    it("leaves the initial loading state when the column IPC request times out", async () => {
        vi.useFakeTimers();
        try {
            const columns = deferred<never>();
            invoke_mock.mockImplementation((command) => {
                if (command === "get_columns") return columns.promise;
                return Promise.resolve();
            });
            const store = create_store();

            store.get_columns();
            await vi.advanceTimersByTimeAsync(15_000);

            expect(store.column_fetch_finish).toBe(true);
            expect(store.column_fetch_error).toBe(true);
        } finally {
            vi.useRealTimers();
        }
    });
});

describe("pending task operations", () => {
    beforeEach(() => {
        invoke_mock.mockReset();
    });

    it("waits for creation before deleting a new task", async () => {
        const add_task = deferred<number>();
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") return add_task.promise;
            if (command === "delete_task") return Promise.resolve();
            if (command === "get_labels") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store();

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        const pending_task_id = store.columns[0].tasks[0].id;
        const deletion = store.delete_task(pending_task_id);

        expect(store.columns[0].tasks).toHaveLength(0);
        add_task.resolve(42);

        await expect(creation).resolves.toBe(true);
        await expect(deletion).resolves.toBe(true);
        expect(invoke_mock).toHaveBeenCalledWith("delete_task", { taskId: 42, expectedBoardId: 7 });
    });

    it("does not restore a pending task when its creation fails", async () => {
        const add_task = deferred<number>();
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") return add_task.promise;
            if (command === "get_labels") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store();

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        const deletion = store.delete_task(store.columns[0].tasks[0].id);
        add_task.reject(new Error("creation failed"));

        await expect(creation).resolves.toBe(false);
        await expect(deletion).resolves.toBe(false);
        expect(store.columns[0].tasks).toHaveLength(0);
        expect(invoke_mock).not.toHaveBeenCalledWith("delete_task", expect.anything());
    });

    it("uses the persisted source ID when duplicating a new task", async () => {
        const first_add = deferred<number>();
        let add_count = 0;
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") {
                add_count += 1;
                return add_count === 1 ? first_add.promise : Promise.resolve(43);
            }
            if (command === "get_labels") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store();

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        const pending_task_id = store.columns[0].tasks[0].id;
        const duplication = store.duplicate_task(pending_task_id);
        first_add.resolve(42);

        await expect(creation).resolves.toBe(true);
        await expect(duplication).resolves.toBe(true);
        expect(invoke_mock).toHaveBeenCalledWith(
            "add_task",
            expect.objectContaining({ afterTaskId: 42 }),
        );
        expect(store.columns[0].tasks.map((task) => task.id)).toEqual([
            "task_42",
            "task_43",
        ]);
    });

    it("waits for creation before updating a new task", async () => {
        const add_task = deferred<number>();
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") return add_task.promise;
            if (command === "update_task") return Promise.resolve();
            if (command === "get_labels") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store();

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        const pending_task = store.columns[0].tasks[0];
        const update = store.update_task(pending_task.id, {
            ...pending_task,
            title: "Edited task",
        });
        add_task.resolve(42);

        await expect(creation).resolves.toBe(true);
        await expect(update).resolves.toBe(true);
        expect(invoke_mock).toHaveBeenCalledWith("update_task", {
            taskId: 42,
            task: expect.objectContaining({ id: 42, title: "Edited task" }),
            expectedBoardId: 7,
        });
    });

    it("resolves a temporary ID retained by a dialog after creation", async () => {
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") return Promise.resolve(42);
            if (command === "get_labels") return Promise.resolve([]);
            if (command === "save_task_template") {
                return Promise.resolve({
                    id: 7,
                    name: "Template",
                    task: {
                        id: 42,
                        title: "New task",
                        description: "",
                        color: "",
                        start_time: 0,
                        due_time: undefined,
                        labels: [],
                        items: [],
                        recurrence: undefined,
                    },
                });
            }
            return Promise.resolve();
        });
        const store = create_store();

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        const retained_pending_id = store.columns[0].tasks[0].id;
        await expect(creation).resolves.toBe(true);

        await expect(store.save_task_template(retained_pending_id, "Template")).resolves.toBe(
            true,
        );
        expect(invoke_mock).toHaveBeenCalledWith("save_task_template", {
            taskId: 42,
            name: "Template",
            expectedBoardId: 7,
        });
    });

    it("finds a created task through its retained temporary ID", async () => {
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") return Promise.resolve(42);
            if (command === "delete_task") return Promise.resolve();
            if (command === "get_labels") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store();

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        const retained_pending_id = store.columns[0].tasks[0].id;
        await expect(creation).resolves.toBe(true);

        await expect(store.delete_task(retained_pending_id)).resolves.toBe(true);
        expect(store.columns[0].tasks).toHaveLength(0);
        expect(invoke_mock).toHaveBeenCalledWith("delete_task", { taskId: 42, expectedBoardId: 7 });
    });

    it("persists a drag made before creation finishes", async () => {
        const add_task = deferred<number>();
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") return add_task.promise;
            if (command === "move_task") return Promise.resolve();
            if (command === "get_labels") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store(2);

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        expect(store.move_task(0, 0, 1, null)).toBe(true);
        add_task.resolve(42);

        await expect(creation).resolves.toBe(true);
        await new Promise((resolve) => setTimeout(resolve, 350));
        expect(invoke_mock).toHaveBeenCalledWith("move_task", {
            taskId: 42,
            toColumnId: 2,
            beforeTaskId: null,
            expectedBoardId: 7,
        });
    });

    it("keeps a queued pending-task deletion bound to the board where it began", async () => {
        const add_task = deferred<number>();
        invoke_mock.mockImplementation((command) => {
            if (command === "add_task") return add_task.promise;
            if (command === "delete_task" || command === "switch_board") return Promise.resolve();
            if (command === "get_columns") return Promise.resolve([]);
            if (command === "get_labels" || command === "get_task_templates" || command === "get_expired_tasks") return Promise.resolve([]);
            return Promise.resolve();
        });
        const store = create_store();
        store.boards = [
            { id: 7, name: "First", task_count: 0, shared_role: "owner", sync_status: "synced", sync_revision: 0 },
            { id: 9, name: "Second", task_count: 0, shared_role: "owner", sync_status: "synced", sync_revision: 0 },
        ];

        const creation = store.add_new_task("column_1", create_task("", "New task"));
        const deletion = store.delete_task(store.columns[0].tasks[0].id);
        await store.switch_board(9);
        add_task.resolve(42);

        await expect(creation).resolves.toBe(true);
        await expect(deletion).resolves.toBe(false);
        expect(invoke_mock).toHaveBeenCalledWith("delete_task", { taskId: 42, expectedBoardId: 7 });
    });

    it("keeps a debounced task move bound to the board where it began", async () => {
        vi.useFakeTimers();
        try {
            const add_task = deferred<number>();
            invoke_mock.mockImplementation((command) => {
                if (command === "add_task") return add_task.promise;
                if (command === "move_task" || command === "switch_board") return Promise.resolve();
                if (command === "get_columns") return Promise.resolve([]);
                if (command === "get_labels" || command === "get_task_templates" || command === "get_expired_tasks") return Promise.resolve([]);
                return Promise.resolve();
            });
            const store = create_store(2);
            store.boards = [
                { id: 7, name: "First", task_count: 0, shared_role: "owner", sync_status: "synced", sync_revision: 0 },
                { id: 9, name: "Second", task_count: 0, shared_role: "owner", sync_status: "synced", sync_revision: 0 },
            ];
            const creation = store.add_new_task("column_1", create_task("", "Task"));

            expect(store.move_task(0, 0, 1, null)).toBe(true);
            // The timer has fired and is now waiting for the temporary card to
            // receive its database ID. Switching must not retarget that IPC.
            await vi.advanceTimersByTimeAsync(300);
            await store.switch_board(9);
            add_task.resolve(42);
            await expect(creation).resolves.toBe(true);
            await Promise.resolve();

            expect(invoke_mock).toHaveBeenCalledWith("move_task", {
                taskId: 42,
                toColumnId: 2,
                beforeTaskId: null,
                expectedBoardId: 7,
            });
        } finally {
            vi.useRealTimers();
        }
    });
});

describe("column sorting", () => {
    it("sorts due dates in either direction and leaves undated tasks last", () => {
        const store = create_store();
        const undated = create_task("task_1", "Undated");
        const later = create_task("task_2", "Later", "", "", undefined, new Date(2026, 7, 20));
        const earlier = create_task("task_3", "Earlier", "", "", undefined, new Date(2026, 7, 10));
        store.columns[0].tasks = [undated, later, earlier];

        store.columns[0].sort_order = "due_date_asc";
        store.sort_column_tasks(store.columns[0]);
        expect(store.columns[0].tasks.map((task) => task.id)).toEqual(["task_3", "task_2", "task_1"]);

        store.columns[0].sort_order = "due_date_desc";
        store.sort_column_tasks(store.columns[0]);
        expect(store.columns[0].tasks.map((task) => task.id)).toEqual(["task_2", "task_3", "task_1"]);
    });
});
