import { describe, expect, it } from "vite-plus/test";

import { create_column, type SortRule } from "../type/column.svelte";
import { create_task, type Task } from "../type/task.svelte";
import { build_share_snapshot } from "../share";
import { sort_rules_hint, sorted_column_tasks } from "./column-sort";

function task(id: number, fields: Partial<Task> = {}): Task {
  return { ...create_task(`task_${id}`, `Task ${id}`), ...fields };
}

function sorted_ids(tasks: Task[], sort_rules: SortRule[]): string[] {
  const column = create_column("column_1", "Todo", "", tasks, sort_rules);
  return sorted_column_tasks(column).map((item) => item.id);
}

describe("column sorting", () => {
  it("keeps the manual order when no rules apply", () => {
    const tasks = [task(3), task(1), task(2)];
    const column = create_column("column_1", "Todo", "", tasks);
    expect(sorted_column_tasks(column)).toBe(tasks);
    expect(sorted_ids(tasks, [{ field: "effort", direction: "asc" }])).toEqual([
      "task_3",
      "task_1",
      "task_2",
    ]);
  });

  it("sorts dates either way with empty values last and pinned tasks first", () => {
    const tasks = [
      task(1),
      task(2, { due_time: new Date(2026, 7, 20, 9) }),
      task(3, { due_time: new Date(2026, 7, 10, 9) }),
      task(4, { pinned: true }),
    ];
    expect(
      sorted_ids(tasks, [{ field: "due_time", direction: "asc" }]),
    ).toEqual(["task_4", "task_3", "task_2", "task_1"]);
    expect(
      sorted_ids(tasks, [{ field: "due_time", direction: "desc" }]),
    ).toEqual(["task_4", "task_2", "task_3", "task_1"]);
  });

  it("orders a date-only due date after times on the same day", () => {
    const tasks = [
      task(1, { due_time: new Date(2026, 7, 10) }),
      task(2, { due_time: new Date(2026, 7, 10, 18) }),
      task(3, { due_time: new Date(2026, 7, 10), due_at_midnight: true }),
    ];
    expect(
      sorted_ids(tasks, [{ field: "due_time", direction: "asc" }]),
    ).toEqual(["task_3", "task_2", "task_1"]);
  });

  it("sorts by start, creation, title and checklist progress", () => {
    const items = (done: number, total: number) =>
      Array.from({ length: total }, (_, index) => ({
        id: String(index),
        text: "",
        completed: index < done,
      }));
    const tasks = [
      task(1, {
        title: "beta",
        start_time: new Date(30),
        created_time: new Date(10),
        items: items(1, 2),
      }),
      task(2, {
        title: "Alpha",
        start_time: new Date(10),
        created_time: new Date(30),
        items: items(2, 2),
      }),
      task(3, {
        title: "",
        start_time: undefined,
        created_time: undefined,
        items: [],
      }),
    ];
    const by = (field: string, direction: "asc" | "desc" = "asc") =>
      sorted_ids(tasks, [{ field, direction }]);
    expect(by("start_time")).toEqual(["task_2", "task_1", "task_3"]);
    expect(by("created_time", "desc")).toEqual(["task_2", "task_1", "task_3"]);
    expect(by("title")).toEqual(["task_2", "task_1", "task_3"]);
    expect(by("title", "desc")).toEqual(["task_1", "task_2", "task_3"]);
    expect(by("checklist_progress")).toEqual(["task_1", "task_2", "task_3"]);
  });

  it("sorts by priority either way with unset priorities last", () => {
    const tasks = [
      task(1),
      task(2, { priority: "low" }),
      task(3, { priority: "urgent" }),
      task(4, { priority: "medium" }),
      task(5, { priority: "high" }),
    ];
    expect(
      sorted_ids(tasks, [{ field: "priority", direction: "desc" }]),
    ).toEqual(["task_3", "task_5", "task_4", "task_2", "task_1"]);
    expect(
      sorted_ids(tasks, [{ field: "priority", direction: "asc" }]),
    ).toEqual(["task_2", "task_4", "task_5", "task_3", "task_1"]);
  });

  it("compares titles by Unicode code point", () => {
    const tasks = [
      task(1, { title: "😀" }),
      task(2, { title: "中" }),
      task(3, { title: "～" }),
    ];
    // U+1F600 sorts after U+FF5E although its UTF-16 lead unit is smaller.
    expect(sorted_ids(tasks, [{ field: "title", direction: "asc" }])).toEqual([
      "task_2",
      "task_3",
      "task_1",
    ]);
  });

  it("uses later levels and then the manual order to break ties", () => {
    const day = new Date(2026, 7, 10, 9);
    const tasks = [
      task(1, { due_time: day, title: "b" }),
      task(2, { due_time: day, title: "a" }),
      task(3, { due_time: day, title: "a" }),
    ];
    expect(
      sorted_ids(tasks, [
        { field: "due_time", direction: "asc" },
        { field: "title", direction: "asc" },
      ]),
    ).toEqual(["task_2", "task_3", "task_1"]);
  });

  it("describes up to three levels in the active language", () => {
    expect(sort_rules_hint([])).toBeUndefined();
    expect(sort_rules_hint([{ field: "due_time", direction: "asc" }])).toBe(
      "Sorted by Due date: Earliest first. Click to change.",
    );
    expect(
      sort_rules_hint([
        { field: "title", direction: "desc" },
        { field: "created_time", direction: "desc" },
        { field: "checklist_progress", direction: "asc" },
      ]),
    ).toBe(
      "Sorted by Title: Z to A, then Creation time: Newest first, then Checklist progress: Least complete first. Click to change.",
    );
  });

  it("publishes shares in display order", () => {
    const column = create_column(
      "column_1",
      "Todo",
      "",
      [task(1, { title: "b" }), task(2, { title: "a" })],
      [{ field: "title", direction: "asc" }],
    );
    const snapshot = build_share_snapshot(
      [column],
      [column.id],
      ["task_1", "task_2"],
      "Board",
    );
    expect(snapshot.columns[0].tasks.map((item) => item.id)).toEqual([2, 1]);
  });
});
