import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), warn: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("$lib/logger", () => ({ logger: { warn: mocks.warn } }));

import {
  clamp_floating_zoom,
  forget_floating_task,
  open_floating_task,
  read_floating_tasks,
  read_floating_zoom,
  restore_floating_tasks,
  write_floating_zoom,
} from "./floating-tasks";

describe("floating task windows", () => {
  let storage: Map<string, string>;

  beforeEach(() => {
    vi.clearAllMocks();
    storage = new Map();
    vi.stubGlobal("window", {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
        removeItem: (key: string) => storage.delete(key),
      },
    });
    mocks.invoke.mockResolvedValue(undefined);
  });
  afterEach(() => vi.unstubAllGlobals());

  it("remembers each opened task once and forgets closed ones", async () => {
    await open_floating_task({ boardId: 1, taskId: 7 });
    await open_floating_task({ boardId: 1, taskId: 7 });
    await open_floating_task({ boardId: 2, taskId: 7 });

    expect(mocks.invoke).toHaveBeenCalledWith("open_floating_task", {
      boardId: 1,
      taskId: 7,
    });
    expect(read_floating_tasks()).toEqual([
      { boardId: 1, taskId: 7 },
      { boardId: 2, taskId: 7 },
    ]);

    forget_floating_task({ boardId: 1, taskId: 7 });
    expect(read_floating_tasks()).toEqual([{ boardId: 2, taskId: 7 }]);
  });

  it("does not remember a window that failed to open", async () => {
    mocks.invoke.mockRejectedValueOnce({ code: "TASK_NOT_FOUND" });
    await expect(open_floating_task({ boardId: 1, taskId: 7 })).rejects.toEqual(
      { code: "TASK_NOT_FOUND" },
    );
    expect(read_floating_tasks()).toEqual([]);
  });

  it("ignores malformed storage", () => {
    storage.set(
      "cardbe.floating-tasks",
      JSON.stringify([{ boardId: 1, taskId: 2 }, { boardId: "1" }, null, 3]),
    );
    expect(read_floating_tasks()).toEqual([{ boardId: 1, taskId: 2 }]);

    storage.set("cardbe.floating-tasks", "{not json");
    expect(read_floating_tasks()).toEqual([]);
  });

  it("keeps zoom per task within range and clears it on close", () => {
    const ref = { boardId: 1, taskId: 7 };
    expect(read_floating_zoom(ref)).toBe(1);
    expect(clamp_floating_zoom(1.2 + 0.1)).toBe(1.3);
    expect(clamp_floating_zoom(0.1)).toBe(0.6);
    expect(clamp_floating_zoom(5)).toBe(2);

    write_floating_zoom(ref, 1.5);
    expect(read_floating_zoom(ref)).toBe(1.5);
    expect(read_floating_zoom({ boardId: 2, taskId: 7 })).toBe(1);
    storage.set("cardbe.floating-task-zoom.1.7", "nonsense");
    expect(read_floating_zoom(ref)).toBe(1);

    write_floating_zoom(ref, 1.5);
    forget_floating_task(ref);
    expect(read_floating_zoom(ref)).toBe(1);
  });

  it("restores windows and drops tasks or boards that no longer exist", async () => {
    storage.set(
      "cardbe.floating-tasks",
      JSON.stringify([
        { boardId: 1, taskId: 1 },
        { boardId: 1, taskId: 2 },
        { boardId: 3, taskId: 1 },
        { boardId: 1, taskId: 4 },
      ]),
    );
    mocks.invoke
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce({ code: "TASK_NOT_FOUND" })
      .mockRejectedValueOnce({ code: "BOARD_NOT_FOUND" })
      .mockRejectedValueOnce({ code: "INTERNAL_ERROR" });

    await restore_floating_tasks();

    expect(mocks.invoke).toHaveBeenCalledTimes(4);
    // Unexpected failures keep the entry so a later launch can retry it.
    expect(read_floating_tasks()).toEqual([
      { boardId: 1, taskId: 1 },
      { boardId: 1, taskId: 4 },
    ]);
    expect(mocks.warn).toHaveBeenCalledOnce();
  });
});
