import { beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import type { AllTaskItem } from "../../board-base.svelte";
import { task_item_key, TaskActions } from "./task-actions.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("svelte-sonner", () => ({ toast: { error: vi.fn() } }));

function item(archived = false): AllTaskItem {
  return {
    board_id: 7,
    board_name: "Board",
    column_name: "Todo",
    archived_at: archived ? new Date(1) : undefined,
    task: {
      id: "task_3",
      title: "Task",
      color: "",
      start_time: undefined,
      due_time: undefined,
      labels: [],
      recurrence: undefined,
    },
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

it("keeps archived copies apart from the task with the same ID", () => {
  expect(task_item_key(item())).not.toBe(task_item_key(item(true)));
});

it("writes the task's own board and reloads once the action ends", async () => {
  const after = vi.fn();
  const actions = new TaskActions(after);
  let finish!: () => void;
  vi.mocked(invoke).mockReturnValue(
    new Promise<void>((resolve) => (finish = resolve)),
  );
  actions.complete(item());
  // A second click while the first is running is ignored.
  actions.complete(item());
  expect(actions.is_busy(item())).toBe(true);
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(invoke).toHaveBeenCalledWith("complete_board_tasks", {
    boardId: 7,
    taskIds: [3],
  });
  finish();
  await vi.waitFor(() => expect(after).toHaveBeenCalledTimes(1));
  expect(actions.is_busy(item())).toBe(false);
});

it("reopens an archived task in an open column it picks", async () => {
  const after = vi.fn();
  const actions = new TaskActions(after);
  vi.mocked(invoke).mockResolvedValueOnce([
    { id: 1, name: "Todo", color: "", tasks: [], done: false },
    { id: 2, name: "Done", color: "", tasks: [], done: true },
  ]);
  actions.ask_to_reopen(item(true));
  await vi.waitFor(() => expect(actions.reopen_open).toBe(true));
  expect(actions.reopen_columns.map((column) => column.name)).toEqual(["Todo"]);
  // Picking a column is not a write yet.
  expect(after).not.toHaveBeenCalled();

  vi.mocked(invoke).mockResolvedValueOnce(undefined);
  actions.reopen("column_1");
  await vi.waitFor(() => expect(after).toHaveBeenCalledTimes(1));
  expect(invoke).toHaveBeenLastCalledWith("reopen_board_tasks", {
    boardId: 7,
    taskIds: [3],
    columnId: 1,
  });
});
