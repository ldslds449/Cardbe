import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import { isPermissionGranted } from "@tauri-apps/plugin-notification";
import type { TaskExplorerQuery } from "./utils/task-explorer";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";
import { applyLanguagePreference, language } from "$lib/i18n";

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

describe("BoardStore disposal", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("removes recurring work and listeners and prevents delayed initialization from restarting them", async () => {
    vi.useFakeTimers();
    const dev_window = new EventTarget();
    Object.assign(dev_window, { setInterval, clearInterval });
    const dev_document = new EventTarget();
    Object.assign(dev_document, { visibilityState: "visible" });
    vi.stubGlobal("window", dev_window);
    vi.stubGlobal("document", dev_document);
    invoke_mock.mockReset();
    invoke_mock.mockImplementation((command) => {
      if (command === "get_settings") {
        return Promise.resolve({ notify_enabled: true });
      }
      if (command === "take_recovery_messages") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    vi.mocked(isPermissionGranted).mockResolvedValue(true);
    const store = new BoardStore();
    vi.spyOn(store, "get_boards").mockResolvedValue(undefined);
    vi.spyOn(store, "update_labels").mockImplementation(() => {});
    vi.spyOn(store, "get_columns").mockImplementation(() => {});
    vi.spyOn(store, "get_task_templates").mockImplementation(() => {});
    const remove_online = vi.spyOn(dev_window, "removeEventListener");
    const remove_visibility = vi.spyOn(dev_document, "removeEventListener");
    await store.init();
    expect(vi.getTimerCount()).toBe(2);
    const boards_ready = deferred<void>();
    vi.mocked(store.get_boards).mockReturnValueOnce(boards_ready.promise);
    const initializing = store.init();
    store.dispose();
    store.dispose();
    expect(vi.getTimerCount()).toBe(0);
    expect(remove_online).toHaveBeenCalledWith("online", expect.any(Function));
    expect(remove_visibility).toHaveBeenCalledWith(
      "visibilitychange",
      expect.any(Function),
    );
    invoke_mock.mockClear();
    dev_window.dispatchEvent(new Event("online"));
    dev_document.dispatchEvent(new Event("visibilitychange"));
    boards_ready.resolve();
    await initializing;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(vi.getTimerCount()).toBe(0);
    expect(invoke_mock.mock.calls.map(([command]) => command)).toEqual([
      "get_settings",
      "take_recovery_messages",
    ]);
  });
});

describe("cross-board task moves", () => {
  it("keeps the selected board and refreshes all tasks when move summaries arrive after a board switch", async () => {
    invoke_mock.mockReset();
    const store = create_store();
    const summaries = deferred<{
      boards: typeof store.boards;
      active_board_id: number;
    }>();
    invoke_mock.mockImplementation((command) => {
      if (command === "list_all_tasks") {
        return Promise.resolve({ items: [], next_cursor: null });
      }
      if (command === "get_boards") {
        return summaries.promise;
      }
      if (
        command === "get_columns" ||
        command === "get_labels" ||
        command === "get_task_templates" ||
        command === "search_tasks"
      ) {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    await store.search_all_tasks({
      query: "",
      board_ids: null,
      column_id: null,
      status: "active",
      sort: "title",
      due: "all",
      due_start: null,
      due_end: null,
      archive_start: null,
      archive_end: null,
      now: Date.now(),
    });
    const moving = store.move_task_to_board("task_42", 8, 3);
    await vi.waitFor(() =>
      expect(invoke_mock).toHaveBeenCalledWith("get_boards"),
    );
    await expect(store.switch_board(9)).resolves.toBe(true);
    summaries.resolve({ boards: store.boards, active_board_id: 7 });
    await expect(moving).resolves.toBe(true);
    expect(store.active_board_id).toBe(9);
    expect(
      invoke_mock.mock.calls.filter(
        ([command]) => command === "list_all_tasks",
      ),
    ).toHaveLength(2);
  });

  it("preserves undo for an edit completed while board summaries are refreshing", async () => {
    invoke_mock.mockReset();
    const store = create_store();
    store.can_undo = true;
    const summaries = deferred<{
      boards: typeof store.boards;
      active_board_id: number;
    }>();
    invoke_mock.mockImplementation((command) => {
      if (command === "get_boards") {
        return summaries.promise;
      }
      if (
        command === "get_columns" ||
        command === "get_labels" ||
        command === "search_tasks"
      ) {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const moving = store.move_task_to_board("task_42", 8, 3);
    await vi.waitFor(() =>
      expect(invoke_mock).toHaveBeenCalledWith("get_boards"),
    );
    expect(store.can_undo).toBe(false);
    store.can_undo = true;
    summaries.resolve({ boards: store.boards, active_board_id: 7 });
    await expect(moving).resolves.toBe(true);
    expect(store.can_undo).toBe(true);
  });

  it("passes explicit source and destination IDs and clears undo after success", async () => {
    invoke_mock.mockReset();
    const store = create_store();
    store.can_undo = true;
    invoke_mock.mockImplementation((command) => {
      if (command === "list_all_tasks") {
        return Promise.resolve({ items: [], next_cursor: null });
      }
      if (command === "get_boards") {
        return Promise.resolve({ boards: store.boards, active_board_id: 7 });
      }
      if (
        command === "get_columns" ||
        command === "get_labels" ||
        command === "search_tasks"
      ) {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    await store.search_all_tasks({
      query: "",
      board_ids: null,
      column_id: null,
      status: "active",
      sort: "title",
      due: "all",
      due_start: null,
      due_end: null,
      archive_start: null,
      archive_end: null,
      now: Date.now(),
    });
    const initial_list_requests = invoke_mock.mock.calls.filter(
      ([command]) => command === "list_all_tasks",
    ).length;
    await expect(store.move_task_to_board("task_42", 8, 3)).resolves.toBe(true);
    expect(invoke_mock).toHaveBeenCalledWith("move_task_to_board", {
      expectedBoardId: 7,
      taskId: 42,
      targetBoardId: 8,
      targetColumnId: 3,
    });
    expect(store.can_undo).toBe(false);
    expect(
      invoke_mock.mock.calls.filter(
        ([command]) => command === "list_all_tasks",
      ),
    ).toHaveLength(initial_list_requests + 1);
    await Promise.resolve();
    await Promise.resolve();
  });
});

describe("language persistence", () => {
  beforeEach(() => {
    invoke_mock.mockReset();
    applyLanguagePreference("en");
  });
  afterEach(() => applyLanguagePreference("system"));

  it("applies a language only after settings are saved", async () => {
    const saved = deferred<void>();
    invoke_mock.mockReturnValue(saved.promise);
    const store = new BoardStore();
    const pending = store.set_language("zh-TW");
    expect(language.preference).toBe("en");
    saved.resolve();
    expect(await pending).toBe(true);
    expect(invoke_mock).toHaveBeenCalledWith("set_language", {
      language: "zh-TW",
    });
    expect(language.preference).toBe("zh-TW");
  });
  it("keeps the previous language when persistence fails", async () => {
    invoke_mock.mockRejectedValue(new Error("disk unavailable"));
    expect(await new BoardStore().set_language("zh-TW")).toBe(false);
    expect(language.preference).toBe("en");
  });
});

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
  store.boards = [
    {
      id: 7,
      name: "Test board",
      task_count: 0,
      shared_role: "owner",
      sync_status: "synced",
      sync_revision: 0,
    },
  ];
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

describe("Iroh sync", () => {
  beforeEach(() => invoke_mock.mockReset());
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("restores revoked access without syncing an unresolved conflict", async () => {
    const store = create_store();
    store.boards[0].shared_role = "editor";
    store.boards[0].sync_status = "conflict";
    const saved = { ...store.boards[0] };
    invoke_mock.mockImplementation((command) => {
      if (command === "get_boards") {
        return Promise.resolve({ boards: [saved], active_board_id: 7 });
      }
      if (command === "request_iroh_board_access") {
        return Promise.reject("ACCESS_REVOKED:Access was declined or revoked.");
      }
      return Promise.resolve();
    });
    await store.sync_iroh_board(7, true, true);
    expect(store.iroh_access_removed[7]).toBe(true);

    // The approval response refreshes permissions while retaining both versions.
    store.apply_iroh_conflict_refresh({ ...saved, shared_role: "viewer" });
    invoke_mock.mockClear();
    vi.mocked(toast.error).mockClear();
    await store.restore_iroh_access(7);

    expect(invoke_mock).not.toHaveBeenCalled();
    expect(store.iroh_access_removed[7]).toBe(false);
    expect(store.iroh_access_error[7]).toBeUndefined();
    expect(store.iroh_sync_error[7]).toBeUndefined();
    expect(store.boards[0].sync_status).toBe("conflict");
    expect(store.boards[0].shared_role).toBe("viewer");
    expect(toast.success).toHaveBeenCalledWith(
      "Access restored. Resolve the conflict before syncing.",
    );
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("syncs after access is restored when there is no conflict", async () => {
    const store = create_store();
    const sync = vi.spyOn(store, "sync_iroh_board").mockResolvedValue(true);
    await store.restore_iroh_access(7);
    expect(sync).toHaveBeenCalledWith(7, true);
    expect(toast.success).toHaveBeenCalledWith("Access restored");
  });

  it("lets a pending board sync after checking two conflicting boards", async () => {
    vi.stubGlobal("document", { visibilityState: "visible" });
    vi.spyOn(Date, "now").mockReturnValue(10_000);
    const store = create_store();
    store.active_board_id = null;
    store.boards = [7, 8, 9].map((id) => ({
      ...store.boards[0],
      id,
      shared_role: "editor" as const,
      sync_status: id === 9 ? "pending" : "conflict",
    }));
    store.iroh_last_synced_at = { 7: 100, 8: 100, 9: 200 };
    invoke_mock.mockImplementation((command) => {
      if (command === "request_iroh_board_access") {
        return Promise.resolve([true, "editor"]);
      }
      if (command === "sync_iroh_board") {
        return Promise.resolve({
          board: {
            ...store.boards.find((board) => board.id === 9),
            sync_status: "synced",
          },
          content_changed: false,
        });
      }
      return Promise.resolve();
    });
    store.trigger_iroh_background_sync();
    expect(
      invoke_mock.mock.calls
        .filter(([command]) => command === "request_iroh_board_access")
        .map(([, args]) => args),
    ).toEqual([
      { boardId: 7, requestApproval: false },
      { boardId: 8, requestApproval: false },
    ]);
    expect(invoke_mock).not.toHaveBeenCalledWith(
      "sync_iroh_board",
      expect.anything(),
    );
    await Promise.resolve();
    expect(store.iroh_last_synced_at).toEqual({ 7: 100, 8: 100, 9: 200 });
    store.trigger_iroh_background_sync();
    await vi.waitFor(() =>
      expect(invoke_mock).toHaveBeenCalledWith("sync_iroh_board", {
        boardId: 9,
      }),
    );
    expect(
      store.boards
        .filter((board) => board.sync_status === "conflict")
        .map((board) => board.id),
    ).toEqual([7, 8]);
  });

  it("retries a failed viewer immediately after connection settings change", async () => {
    vi.stubGlobal("document", { visibilityState: "visible" });
    const store = create_store();
    store.boards[0].shared_role = "viewer";
    store.active_board_id = null;
    store.iroh_last_synced_at[7] = Date.now();
    const synced = { ...store.boards[0], sync_status: "synced" as const };
    let attempts = 0;
    invoke_mock.mockImplementation((command) => {
      if (command === "sync_iroh_board") {
        attempts += 1;
        return attempts === 1
          ? Promise.reject("Owner unreachable")
          : Promise.resolve({ board: synced, content_changed: false });
      }
      if (command === "get_boards") {
        return Promise.resolve({ boards: store.boards, active_board_id: null });
      }
      return Promise.resolve();
    });
    await store.sync_iroh_board(7, true);
    expect(store.iroh_sync_error[7]).toBe("Owner unreachable");
    store.trigger_iroh_background_sync();
    expect(attempts).toBe(1);
    store.reconnect_iroh_boards();
    await vi.waitFor(() => expect(attempts).toBe(2));
    await vi.waitFor(() => expect(store.iroh_sync_error[7]).toBeUndefined());
    expect(store.boards[0].sync_status).toBe("synced");
  });

  it("reconnects after an old in-flight sync fails without applying its retry delay", async () => {
    vi.stubGlobal("document", { visibilityState: "visible" });
    const store = create_store();
    store.boards[0].shared_role = "editor";
    store.can_undo = true;
    const interrupted = deferred<unknown>();
    let attempts = 0;
    invoke_mock.mockImplementation((command) => {
      if (command === "sync_iroh_board") {
        attempts += 1;
        return attempts === 1
          ? interrupted.promise
          : Promise.resolve({
              board: { ...store.boards[0], sync_status: "synced" },
              content_changed: false,
            });
      }
      return Promise.resolve();
    });
    const original = store.sync_iroh_board(7, true);
    store.reconnect_iroh_boards();
    expect(attempts).toBe(1);
    interrupted.reject("Old endpoint was closed");
    await original;
    await vi.waitFor(() => expect(attempts).toBe(2));
    expect(store.iroh_sync_error[7]).toBeUndefined();
    expect(store.can_undo).toBe(true);
  });

  it("keeps pending edits marked pending if reconnecting also fails", async () => {
    vi.stubGlobal("document", { visibilityState: "visible" });
    const store = create_store();
    store.boards[0].shared_role = "editor";
    store.boards[0].sync_status = "pending";
    const saved = { ...store.boards[0] };
    const interrupted = deferred<unknown>();
    let attempts = 0;
    invoke_mock.mockImplementation((command) => {
      if (command === "sync_iroh_board") {
        attempts += 1;
        return attempts === 1
          ? interrupted.promise
          : Promise.reject("Owner still unreachable");
      }
      if (command === "get_boards") {
        return Promise.resolve({ boards: [saved], active_board_id: 7 });
      }
      return Promise.resolve();
    });
    const original = store.sync_iroh_board(7, true);
    store.reconnect_iroh_boards();
    interrupted.reject("Old endpoint was closed");
    await original;
    await vi.waitFor(() =>
      expect(store.iroh_sync_error[7]).toBe("Owner still unreachable"),
    );
    expect(store.boards[0].sync_status).toBe("pending");
  });

  it("keeps conflict data and sync timestamps unchanged when access is still approved", async () => {
    const store = create_store();
    store.boards[0].shared_role = "editor";
    store.boards[0].sync_status = "conflict";
    store.iroh_last_synced_at[7] = 1000;
    store.can_undo = true;
    invoke_mock.mockResolvedValue([true, "device"]);

    expect(await store.sync_iroh_board(7, true, true)).toBe(true);
    expect(store.boards[0].sync_status).toBe("conflict");
    expect(store.iroh_last_synced_at[7]).toBe(1000);
    expect(store.can_undo).toBe(true);
    expect(invoke_mock).toHaveBeenCalledTimes(1);
    expect(invoke_mock).toHaveBeenCalledWith("request_iroh_board_access", {
      boardId: 7,
      requestApproval: false,
    });
  });

  it("updates restored editor permission while preserving conflict content and sync time", async () => {
    const store = create_store();
    store.boards[0].shared_role = "viewer";
    store.boards[0].sync_status = "conflict";
    store.iroh_last_synced_at[7] = 1000;
    const columns = store.columns;
    invoke_mock.mockResolvedValue([
      true,
      "device",
      { ...store.boards[0], shared_role: "editor" },
    ]);

    expect(await store.sync_iroh_board(7, true, true)).toBe(true);
    expect(store.boards[0].shared_role).toBe("editor");
    expect(store.boards[0].sync_status).toBe("conflict");
    expect(store.columns).toBe(columns);
    expect(store.iroh_last_synced_at[7]).toBe(1000);
  });

  it.each([false, true])(
    "ignores a delayed conflict refresh after resolution (keep local: %s)",
    async (keep_local) => {
      const store = create_store();
      store.active_board_id = null;
      store.boards[0].shared_role = "editor";
      store.boards[0].sync_status = "conflict";
      const stale = { ...store.boards[0], shared_role: "viewer" as const };
      const resolved = {
        ...store.boards[0],
        sync_status: keep_local ? "pending" : "synced",
      };
      const response = deferred<[boolean, string, typeof stale]>();
      invoke_mock.mockImplementation((command) => {
        if (command === "request_iroh_board_access") {
          return response.promise;
        }
        if (command === "get_boards") {
          return Promise.resolve({ boards: [resolved], active_board_id: null });
        }
        return Promise.resolve();
      });
      vi.spyOn(store, "trigger_iroh_background_sync").mockImplementation(
        () => {},
      );

      const checking = store.sync_iroh_board(7, true, true);
      expect(await store.resolve_iroh_conflict(7, keep_local)).toBe(true);
      response.resolve([true, "device", stale]);
      await checking;
      expect(store.boards[0]).toEqual(resolved);

      // The share dialog uses the same guard for its access responses.
      store.apply_iroh_conflict_refresh(stale);
      expect(store.boards[0]).toEqual(resolved);
    },
  );

  it.each(["viewer", "conflict", "pending"])(
    "reports revoked access during silent background checks for %s boards",
    async (state) => {
      vi.stubGlobal("document", { visibilityState: "visible" });
      vi.spyOn(Date, "now").mockReturnValue(1000);
      vi.mocked(toast.error).mockClear();
      const store = create_store();
      store.boards[0].shared_role = state === "viewer" ? "viewer" : "editor";
      store.boards[0].sync_status = state === "viewer" ? "synced" : state;
      store.iroh_last_synced_at[7] = 1000;
      const saved = { ...store.boards[0] };
      invoke_mock.mockImplementation((command) => {
        if (command === "get_boards") {
          return Promise.resolve({ boards: [saved], active_board_id: 7 });
        }
        if (
          command === "request_iroh_board_access" ||
          command === "sync_iroh_board"
        ) {
          return Promise.reject(
            "ACCESS_REVOKED:Access was declined or revoked. You can request access again.",
          );
        }
        return Promise.resolve();
      });

      store.trigger_iroh_background_sync();

      await vi.waitFor(() => expect(store.iroh_access_removed[7]).toBe(true));
      await vi.waitFor(() =>
        expect(store.boards[0].sync_status).toBe(
          state === "conflict" ? "conflict" : "error",
        ),
      );
      expect(store.iroh_access_error[7]).toContain(
        "Access was declined or revoked",
      );
      expect(toast.error).not.toHaveBeenCalled();
      if (state !== "pending") {
        expect(invoke_mock).toHaveBeenCalledWith("request_iroh_board_access", {
          boardId: 7,
          requestApproval: false,
        });
      }
    },
  );

  it("rotates through boards with two slots even when the system reports offline", async () => {
    vi.stubGlobal("navigator", { onLine: false });
    vi.stubGlobal("document", { visibilityState: "visible" });
    const clock = vi.spyOn(Date, "now").mockReturnValue(1000);
    const store = create_store();
    store.active_board_id = null;
    store.boards = [1, 2, 3].map((id) => ({
      ...store.boards[0],
      id,
      shared_role: "editor",
    }));
    const responses = [deferred<unknown>(), deferred<unknown>()];
    const started: number[] = [];
    invoke_mock.mockImplementation((command, args) => {
      if (command !== "sync_iroh_board") {
        return Promise.resolve();
      }
      const id = (args as { boardId: number }).boardId;
      started.push(id);
      return (
        responses[id - 1]?.promise ??
        Promise.resolve({
          board: store.boards.find((board) => board.id === id),
          content_changed: false,
        })
      );
    });

    store.trigger_iroh_background_sync();
    store.trigger_iroh_background_sync();
    expect(started).toEqual([1, 2]);
    responses.forEach((response, index) =>
      response.resolve({
        board: { ...store.boards[index], sync_status: "synced" },
        content_changed: false,
      }),
    );
    await vi.waitFor(() => expect(store.iroh_last_synced_at[2]).toBe(1000));
    clock.mockReturnValue(2000);
    store.trigger_iroh_background_sync();
    expect(started.slice(2)).toEqual([3, 1]);
    await vi.waitFor(() => expect(store.iroh_last_synced_at[3]).toBe(2000));
  });

  it.each([false, true])(
    "only clears local Undo and reloads for changed content (%s)",
    async (content_changed) => {
      const store = create_store();
      store.boards[0].shared_role = "editor";
      store.can_undo = true;
      const reload = vi
        .spyOn(store, "get_columns")
        .mockImplementation(() => {});
      vi.spyOn(store, "update_labels").mockImplementation(() => {});
      vi.spyOn(store, "get_task_templates").mockImplementation(() => {});
      vi.spyOn(store, "get_expired_tasks").mockImplementation(() => {});
      invoke_mock.mockResolvedValue({
        board: { ...store.boards[0] },
        content_changed,
      });

      expect(await store.sync_iroh_board(7, true)).toBe(true);
      expect(store.can_undo).toBe(!content_changed);
      expect(reload).toHaveBeenCalledTimes(content_changed ? 1 : 0);
    },
  );
});

describe("Undo scope and shared Task Explorer updates", () => {
  beforeEach(() => {
    invoke_mock.mockReset();
    vi.mocked(toast.success).mockClear();
  });

  it("rejects a task edit after its board becomes read-only without changing local data", async () => {
    const store = create_store();
    const task = create_task("task_1", "Original");
    store.columns[0].tasks = [task];
    store.boards[0].shared_role = "editor";
    expect(store.can_edit).toBe(true);
    const edited_task = { ...task, title: "Edited" };
    store.boards[0].shared_role = "viewer";
    expect(await store.update_task(task.id, edited_task)).toBe(false);
    expect(store.columns[0].tasks[0].title).toBe("Original");
    expect(invoke_mock).not.toHaveBeenCalled();
  });

  it.each(["editor", "viewer", "shared owner"])(
    "allows Undo for local edits only when a %s board is editable",
    async (role) => {
      const store = create_store();
      if (role === "shared owner") {
        store.boards[0].is_shared = true;
      } else {
        store.boards[0].shared_role = role as "editor" | "viewer";
      }
      invoke_mock.mockImplementation((command) =>
        Promise.resolve(
          command === "add_task" ? 42 : command === "undo" ? false : [],
        ),
      );
      await store.add_new_task("column_1", create_task("", "Task"));
      store.can_undo = true;
      const editable = role !== "viewer";
      expect(store.can_undo).toBe(editable);
      expect(await store.undo()).toBe(editable);
      expect(
        invoke_mock.mock.calls.some(([command]) => command === "undo"),
      ).toBe(editable);
      expect(
        vi
          .mocked(toast.success)
          .mock.calls.some(([, options]) => options?.action),
      ).toBe(editable);
    },
  );

  it.each([false, true])(
    "settings preserve existing Undo availability (%s)",
    async (has_history) => {
      const store = create_store();
      store.can_undo = has_history;
      vi.mocked(isPermissionGranted).mockResolvedValue(true);
      invoke_mock.mockResolvedValue(undefined);
      vi.useFakeTimers();
      await store.set_notify_enabled(true);
      expect(store.notify_enabled).toBe(true);
      expect(store.can_undo).toBe(has_history);
      vi.clearAllTimers();
      vi.useRealTimers();
    },
  );

  it.each(["sync", "push"])(
    "refreshes Explorer after an inactive shared board %s",
    async (event) => {
      const store = create_store();
      const shared = {
        ...store.boards[0],
        id: 9,
        shared_role: "editor" as const,
        is_shared: true,
      };
      store.boards.push(shared);
      const filter: TaskExplorerQuery = {
        query: "",
        board_ids: null,
        column_id: null,
        status: "active",
        sort: "due",
        due: "all",
        due_start: null,
        due_end: null,
        archive_start: null,
        archive_end: null,
        now: 0,
      };
      invoke_mock.mockImplementation((command) => {
        if (command === "list_all_tasks") {
          return Promise.resolve({ items: [], next_cursor: null });
        }
        if (command === "get_boards") {
          return Promise.resolve({ boards: store.boards, active_board_id: 7 });
        }
        return Promise.resolve({ board: shared, content_changed: true });
      });
      await store.search_all_tasks(filter);
      if (event === "sync") {
        await store.sync_iroh_board(9, true);
      } else {
        await store.handle_iroh_remote_push(9, 1);
      }
      expect(
        invoke_mock.mock.calls.filter(
          ([command]) => command === "list_all_tasks",
        ),
      ).toHaveLength(2);
      expect(store.active_board_id).toBe(7);
    },
  );
});

describe("initial board loading", () => {
  beforeEach(() => {
    invoke_mock.mockReset();
  });

  it("loads columns without waiting for an earlier background mission", async () => {
    const labels = deferred<string[]>();
    invoke_mock.mockImplementation((command) => {
      if (command === "get_labels") {
        return labels.promise;
      }
      if (command === "get_columns") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store();

    store.update_labels();
    store.get_columns();

    expect(invoke_mock).toHaveBeenCalledWith("get_columns", {
      expectedBoardId: 7,
    });
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
        if (command === "get_columns") {
          return columns.promise;
        }
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
      if (command === "add_task") {
        return add_task.promise;
      }
      if (command === "delete_task") {
        return Promise.resolve();
      }
      if (command === "get_labels") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store();

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
    const pending_task_id = store.columns[0].tasks[0].id;
    const deletion = store.delete_task(pending_task_id);

    expect(store.columns[0].tasks).toHaveLength(0);
    add_task.resolve(42);

    await expect(creation).resolves.toBe(true);
    await expect(deletion).resolves.toBe(true);
    expect(invoke_mock).toHaveBeenCalledWith("delete_task", {
      taskId: 42,
      expectedBoardId: 7,
    });
  });

  it("does not restore a pending task when its creation fails", async () => {
    const add_task = deferred<number>();
    invoke_mock.mockImplementation((command) => {
      if (command === "add_task") {
        return add_task.promise;
      }
      if (command === "get_labels") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store();

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
    const deletion = store.delete_task(store.columns[0].tasks[0].id);
    add_task.reject(new Error("creation failed"));

    await expect(creation).resolves.toBe(false);
    await expect(deletion).resolves.toBe(false);
    expect(store.columns[0].tasks).toHaveLength(0);
    expect(invoke_mock).not.toHaveBeenCalledWith(
      "delete_task",
      expect.anything(),
    );
  });

  it("uses the persisted source ID when duplicating a new task", async () => {
    const first_add = deferred<number>();
    let add_count = 0;
    invoke_mock.mockImplementation((command) => {
      if (command === "add_task") {
        add_count += 1;
        return add_count === 1 ? first_add.promise : Promise.resolve(43);
      }
      if (command === "get_labels") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store();

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
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
      if (command === "add_task") {
        return add_task.promise;
      }
      if (command === "update_task") {
        return Promise.resolve();
      }
      if (command === "get_labels") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store();

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
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
      if (command === "add_task") {
        return Promise.resolve(42);
      }
      if (command === "get_labels") {
        return Promise.resolve([]);
      }
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

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
    const retained_pending_id = store.columns[0].tasks[0].id;
    await expect(creation).resolves.toBe(true);

    await expect(
      store.save_task_template(retained_pending_id, "Template"),
    ).resolves.toBe(true);
    expect(invoke_mock).toHaveBeenCalledWith("save_task_template", {
      taskId: 42,
      name: "Template",
      expectedBoardId: 7,
    });
  });

  it("finds a created task through its retained temporary ID", async () => {
    invoke_mock.mockImplementation((command) => {
      if (command === "add_task") {
        return Promise.resolve(42);
      }
      if (command === "delete_task") {
        return Promise.resolve();
      }
      if (command === "get_labels") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store();

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
    const retained_pending_id = store.columns[0].tasks[0].id;
    await expect(creation).resolves.toBe(true);

    await expect(store.delete_task(retained_pending_id)).resolves.toBe(true);
    expect(store.columns[0].tasks).toHaveLength(0);
    expect(invoke_mock).toHaveBeenCalledWith("delete_task", {
      taskId: 42,
      expectedBoardId: 7,
    });
  });

  it("persists a drag made before creation finishes", async () => {
    const add_task = deferred<number>();
    invoke_mock.mockImplementation((command) => {
      if (command === "add_task") {
        return add_task.promise;
      }
      if (command === "move_task") {
        return Promise.resolve();
      }
      if (command === "get_labels") {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store(2);

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
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
      if (command === "add_task") {
        return add_task.promise;
      }
      if (command === "delete_task" || command === "switch_board") {
        return Promise.resolve();
      }
      if (command === "get_columns") {
        return Promise.resolve([]);
      }
      if (
        command === "get_labels" ||
        command === "get_task_templates" ||
        command === "get_expired_tasks"
      ) {
        return Promise.resolve([]);
      }
      return Promise.resolve();
    });
    const store = create_store();
    store.boards = [
      {
        id: 7,
        name: "First",
        task_count: 0,
        shared_role: "owner",
        sync_status: "synced",
        sync_revision: 0,
      },
      {
        id: 9,
        name: "Second",
        task_count: 0,
        shared_role: "owner",
        sync_status: "synced",
        sync_revision: 0,
      },
    ];

    const creation = store.add_new_task(
      "column_1",
      create_task("", "New task"),
    );
    const deletion = store.delete_task(store.columns[0].tasks[0].id);
    await store.switch_board(9);
    add_task.resolve(42);

    await expect(creation).resolves.toBe(true);
    await expect(deletion).resolves.toBe(false);
    expect(invoke_mock).toHaveBeenCalledWith("delete_task", {
      taskId: 42,
      expectedBoardId: 7,
    });
  });

  it("keeps a debounced task move bound to the board where it began", async () => {
    vi.useFakeTimers();
    try {
      const add_task = deferred<number>();
      invoke_mock.mockImplementation((command) => {
        if (command === "add_task") {
          return add_task.promise;
        }
        if (command === "move_task" || command === "switch_board") {
          return Promise.resolve();
        }
        if (command === "get_columns") {
          return Promise.resolve([]);
        }
        if (
          command === "get_labels" ||
          command === "get_task_templates" ||
          command === "get_expired_tasks"
        ) {
          return Promise.resolve([]);
        }
        return Promise.resolve();
      });
      const store = create_store(2);
      store.boards = [
        {
          id: 7,
          name: "First",
          task_count: 0,
          shared_role: "owner",
          sync_status: "synced",
          sync_revision: 0,
        },
        {
          id: 9,
          name: "Second",
          task_count: 0,
          shared_role: "owner",
          sync_status: "synced",
          sync_revision: 0,
        },
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
  it("keeps pinned cards first for custom and date sorting", () => {
    const store = create_store();
    const first = create_task("task_1", "First");
    const pinned = create_task("task_2", "Pinned");
    pinned.pinned = true;
    const dated = create_task(
      "task_3",
      "Dated",
      "",
      "",
      undefined,
      new Date(100),
    );
    store.columns[0].tasks = [first, pinned, dated];
    store.sort_column_tasks(store.columns[0]);
    expect(store.columns[0].tasks.map((task) => task.id)).toEqual([
      "task_2",
      "task_1",
      "task_3",
    ]);
    store.columns[0].sort_order = "due_date_asc";
    store.sort_column_tasks(store.columns[0]);
    expect(store.columns[0].tasks.map((task) => task.id)).toEqual([
      "task_2",
      "task_3",
      "task_1",
    ]);
    pinned.pinned = false;
    store.sort_column_tasks(store.columns[0]);
    expect(store.columns[0].tasks.map((task) => task.id)).toEqual([
      "task_3",
      "task_2",
      "task_1",
    ]);
  });
  it("sorts due dates in either direction and leaves undated tasks last", () => {
    const store = create_store();
    const undated = create_task("task_1", "Undated");
    const later = create_task(
      "task_2",
      "Later",
      "",
      "",
      undefined,
      new Date(2026, 7, 20),
    );
    const earlier = create_task(
      "task_3",
      "Earlier",
      "",
      "",
      undefined,
      new Date(2026, 7, 10),
    );
    store.columns[0].tasks = [undated, later, earlier];

    store.columns[0].sort_order = "due_date_asc";
    store.sort_column_tasks(store.columns[0]);
    expect(store.columns[0].tasks.map((task) => task.id)).toEqual([
      "task_3",
      "task_2",
      "task_1",
    ]);

    store.columns[0].sort_order = "due_date_desc";
    store.sort_column_tasks(store.columns[0]);
    expect(store.columns[0].tasks.map((task) => task.id)).toEqual([
      "task_2",
      "task_3",
      "task_1",
    ]);
  });
});
