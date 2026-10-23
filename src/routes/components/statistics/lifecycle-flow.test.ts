import { expect, it } from "vite-plus/test";
import { create_task, type Task } from "../../type/task.svelte";
import { create_column } from "../../type/column.svelte";
import {
  column_flow,
  completions,
  cumulative_flow,
  current_events,
  done_since,
  flow_groups,
} from "./lifecycle-flow";
import type { TaskEvent } from "./task-events";

const DAY = 24 * 60 * 60 * 1000;
const now = new Date(2024, 5, 30, 12);
const ago = (days: number) => new Date(now.getTime() - days * DAY);
const task = (id: string): Task => ({ ...create_task(), id, title: id });
const done = new Set(["done"]);

let next_id = 0;
function event(
  days_ago: number,
  task_id: string,
  kind: TaskEvent["kind"],
  from: string | null,
  to: string | null,
  fields: Partial<TaskEvent> = {},
): TaskEvent {
  next_id++;
  return {
    id: next_id,
    op_id: next_id,
    origin: "local",
    plugin_instance_id: null,
    reverts_op_id: null,
    at: ago(days_ago),
    task_id,
    kind,
    from_column_id: from,
    to_column_id: to,
    ...fields,
  };
}

it("starts after the last reset and drops undone operations", () => {
  const old = event(20, "a", "created", null, "todo");
  const reset = event(10, "task_0", "board_reset", null, null);
  const moved = event(9, "a", "moved", "todo", "doing");
  const undo = event(8, "a", "moved", "doing", "todo", {
    origin: "undo",
    reverts_op_id: moved.op_id,
  });
  const archived = event(7, "a", "archived", "todo", null);
  const events = current_events([old, reset, moved, undo, archived]);
  expect(events.since).toEqual(ago(10));
  expect(events.all).toEqual([moved, undo, archived]);
  expect(events.effective).toEqual([archived]);
  expect(current_events([moved]).since).toEqual(ago(9));
  expect(current_events([]).since).toBeNull();
});

it("completes tasks when they last reach a done column", () => {
  const events = [
    event(10, "a", "created", null, "todo"),
    event(8, "a", "moved", "todo", "done"),
    event(6, "b", "created", null, "doing"),
    event(5, "b", "moved", "doing", "done"),
    event(4, "b", "moved", "done", "doing"),
    event(3, "b", "moved", "doing", "done"),
    event(9, "c", "moved", "doing", "done"),
    event(7, "c", "archived", "done", null),
    event(6, "d", "archived", "doing", null),
    event(2, "e", "moved", "done", "doing"),
  ];
  const since = done_since(events, done);
  expect([...since]).toEqual([
    ["a", ago(8)],
    ["b", ago(3)],
    ["c", ago(9)],
  ]);

  const columns = [
    create_column("doing", "Doing", "", [task("e")]),
    create_column("done", "Done", "", [task("a"), task("b"), task("old")]),
  ];
  const archives = [
    { time: ago(7), task: task("c") },
    { time: ago(6), task: task("d") },
  ];
  const result = completions(archives, columns, done, since);
  expect(result.completed.map((item) => [item.task.id, item.time])).toEqual([
    ["c", ago(9)],
    ["d", ago(6)],
    ["a", ago(8)],
    ["b", ago(3)],
  ]);
  expect(result.active.map((column) => column.id)).toEqual(["doing"]);

  // Restoring into a done column completes the task again from then on.
  expect([
    ...done_since(
      [
        event(5, "f", "archived", "doing", null),
        event(1, "f", "restored", null, "done"),
      ],
      done,
    ),
  ]).toEqual([["f", ago(1)]]);
});

it("measures stays, backflow, reopens and the bottleneck in the range", () => {
  const columns = [
    create_column("todo", "Todo", "", [task("x")]),
    create_column("doing", "Doing", "", []),
    create_column("done", "Done", "", []),
  ];
  const events = [
    event(40, "a", "created", null, "todo"),
    // The stay ends before the range.
    event(35, "a", "moved", "todo", "doing"),
    ...["b", "c", "d"].flatMap((id, i) => [
      event(20, id, "created", null, "todo"),
      event(19 - i, id, "moved", "todo", "doing"),
      event(10, id, "moved", "doing", "done"),
    ]),
    event(9, "c", "moved", "done", "doing"),
    event(8, "c", "moved", "doing", "todo"),
    event(7, "d", "archived", "done", null),
    event(6, "d", "restored", null, "todo"),
    // A stay that began before recording has no known length.
    event(5, "y", "moved", "doing", "todo"),
  ];
  const flow = column_flow(events, columns, done, 30, now);
  expect(
    flow.rows.map((row) => [row.column.id, row.samples, row.backflow]),
  ).toEqual([
    ["todo", 3, 0],
    ["doing", 4, 2],
    ["done", 2, 1],
  ]);
  expect(flow.rows[0].median).toBe(2 * DAY);
  expect(flow.rows[1].median).toBe(7.5 * DAY);
  expect(flow.rows[1].p85).toBe(9 * DAY);
  expect(flow.rows[0].current).toBe(1);
  // Done columns are never the bottleneck.
  expect(flow.bottleneck).toBe("doing");
  expect(flow.completed).toBe(3);
  expect(flow.reopened).toBe(2);

  expect(
    column_flow(events.slice(0, 5), columns, done, 30, now).bottleneck,
  ).toBeNull();
});

it("rebuilds daily column and archive counts backwards", () => {
  const columns = [
    create_column("todo", "Todo", "", [task("a")]),
    create_column("done", "Done", "", [task("b")]),
  ];
  const events = [
    event(3.5, "a", "created", null, "todo"),
    event(3.5, "b", "created", null, "todo"),
    event(2.5, "b", "moved", "todo", "done"),
    event(2.5, "c", "archived", "todo", null),
    event(1.5, "d", "archived", "done", null),
    // Undos really changed the board, so they are replayed too.
    event(1.5, "d", "restored", null, "done", { origin: "undo" }),
    event(0.5, "d", "deleted", "done", null),
    event(0.5, "e", "deleted", null, null),
  ];
  const points = cumulative_flow(events, ago(4), columns, 1, 30, now);
  expect(
    points.map((point) => [point.time, point.columns, point.archived]),
  ).toEqual([
    [ago(4), [1, 1], 1],
    [ago(3), [3, 1], 1],
    [ago(2), [1, 2], 2],
    [ago(1), [1, 2], 2],
    [ago(0), [1, 1], 1],
  ]);
  expect(cumulative_flow([], null, columns, 0, 30, now)).toEqual([]);
});

it("folds later columns into one band but keeps done columns", () => {
  const board = (count: number) =>
    Array.from({ length: count }, (_, i) =>
      create_column(`c${i}`, `C${i}`, "", []),
    );
  const bands = (count: number, done: string[]) =>
    flow_groups(board(count), new Set(done)).map((group) => [
      group.column?.id ?? null,
      group.indexes,
    ]);
  expect(bands(5, [])).toEqual([0, 1, 2, 3, 4].map((i) => [`c${i}`, [i]]));
  expect(bands(7, [])).toEqual([
    ["c0", [0]],
    ["c1", [1]],
    ["c2", [2]],
    ["c3", [3]],
    [null, [4, 5, 6]],
  ]);
  expect(bands(7, ["c6", "c2"])).toEqual([
    ["c0", [0]],
    ["c1", [1]],
    ["c2", [2]],
    [null, [3, 4, 5]],
    ["c6", [6]],
  ]);
  expect(bands(7, ["c4", "c5", "c6", "c0", "c1"])).toEqual([
    ["c0", [0]],
    ["c1", [1]],
    [null, [2, 3]],
    ["c4", [4]],
    ["c5", [5]],
    ["c6", [6]],
  ]);
});
