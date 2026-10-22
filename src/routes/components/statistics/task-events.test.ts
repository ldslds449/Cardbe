import { expect, it, vi } from "vite-plus/test";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  deserialize_task_event,
  load_task_events,
  task_history,
  type TaskEvent,
} from "./task-events";

const moved = {
  id: 3,
  op_id: 2,
  origin: "remote",
  plugin_instance_id: null,
  reverts_op_id: null,
  at: 1_700_000_000_000,
  task_id: 10,
  kind: "moved",
  from_column_id: 1,
  to_column_id: 2,
};

it("deserializes ids and times and skips events from newer versions", async () => {
  expect(deserialize_task_event(moved)).toEqual({
    ...moved,
    at: new Date(1_700_000_000_000),
    task_id: "task_10",
    from_column_id: "column_1",
    to_column_id: "column_2",
  });
  expect(
    deserialize_task_event({ ...moved, kind: "archived", to_column_id: null })
      ?.to_column_id,
  ).toBeNull();

  invoke.mockResolvedValueOnce([
    moved,
    { ...moved, id: 4, kind: "updated" },
    { ...moved, id: 5, origin: "sync" },
  ]);
  const events = await load_task_events(7);
  expect(invoke).toHaveBeenCalledWith("get_task_events", {
    expectedBoardId: 7,
    taskId: null,
  });
  expect(events.map((event) => event.id)).toEqual([3]);
  invoke.mockResolvedValueOnce([]);
  await load_task_events(7, "task_10");
  expect(invoke).toHaveBeenLastCalledWith("get_task_events", {
    expectedBoardId: 7,
    taskId: 10,
  });
});

it("keeps a task's history after the last reset without undone operations", () => {
  let id = 0;
  const event = (
    op_id: number,
    kind: TaskEvent["kind"],
    fields: Partial<TaskEvent> = {},
  ): TaskEvent => ({
    id: ++id,
    op_id,
    origin: "local",
    plugin_instance_id: null,
    reverts_op_id: null,
    at: new Date(op_id),
    task_id: "task_10",
    kind,
    from_column_id: null,
    to_column_id: "column_1",
    ...fields,
  });
  const before_reset = event(1, "created");
  const reset = event(2, "board_reset", {
    task_id: "task_0",
    to_column_id: null,
  });
  const created = event(3, "created");
  const undone = event(4, "moved", {
    from_column_id: "column_1",
    to_column_id: "column_2",
  });
  const undo = event(5, "moved", {
    origin: "undo",
    reverts_op_id: 4,
    from_column_id: "column_2",
    to_column_id: "column_1",
  });
  const other_task = event(6, "moved", { task_id: "task_11" });
  const remote = event(7, "archived", {
    origin: "remote",
    from_column_id: "column_1",
    to_column_id: null,
  });
  const events = [
    before_reset,
    reset,
    created,
    undone,
    undo,
    other_task,
    remote,
  ];
  expect(task_history(events, "task_10")).toEqual({
    entries: [created, remote],
    complete: true,
  });
  expect(task_history([remote], "task_10")).toEqual({
    entries: [remote],
    complete: false,
  });
  expect(task_history([before_reset, reset], "task_10")).toEqual({
    entries: [],
    complete: false,
  });
});
