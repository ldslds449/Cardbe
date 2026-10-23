import { describe, expect, it } from "vite-plus/test";

import type { AllTaskItem } from "../../board-base.svelte";
import type { TaskPriority } from "../../type/task.svelte";
import { group_smart_view_items } from "./smart-view-groups";

function item(
  id: number,
  board: [number, string],
  priority?: TaskPriority,
  due_time?: Date,
): AllTaskItem {
  return {
    board_id: board[0],
    board_name: board[1],
    column_name: "Todo",
    archived_at: undefined,
    task: {
      id: `task_${id}`,
      title: `Task ${id}`,
      color: "",
      start_time: undefined,
      due_time,
      labels: [],
      recurrence: undefined,
      priority,
    },
  };
}

const keys = (groups: ReturnType<typeof group_smart_view_items>) =>
  groups.map((group) => [group.key, group.items.map((i) => i.task.id)]);

describe("smart view groups", () => {
  const now = new Date(2026, 2, 10, 15, 30);
  const work: [number, string] = [1, "Work"];
  const home: [number, string] = [2, "Home"];

  it("keeps one unlabelled group, or none when empty", () => {
    const items = [item(1, work), item(2, home)];
    expect(group_smart_view_items(items, "none", now)).toEqual([
      { key: "all", label: null, items },
    ]);
    expect(group_smart_view_items([], "none", now)).toEqual([]);
  });

  it("orders boards by name and keeps item order within groups", () => {
    const items = [item(1, work), item(2, home), item(3, work)];
    expect(keys(group_smart_view_items(items, "board", now))).toEqual([
      ["board_2", ["task_2"]],
      ["board_1", ["task_1", "task_3"]],
    ]);
  });

  it("orders priorities from most important to none", () => {
    const items = [
      item(1, work),
      item(2, work, "low"),
      item(3, work, "urgent"),
      item(4, work, "low"),
    ];
    expect(keys(group_smart_view_items(items, "priority", now))).toEqual([
      ["urgent", ["task_3"]],
      ["low", ["task_2", "task_4"]],
      ["none", ["task_1"]],
    ]);
  });

  it("buckets due dates like the Today view", () => {
    const items = [
      item(1, work, undefined, new Date(2026, 2, 30)),
      item(2, work),
      item(3, work, undefined, new Date(2026, 2, 10, 18)),
      item(4, work, undefined, new Date(2026, 2, 9)),
      item(5, work, undefined, new Date(2026, 2, 12)),
      // A date-only due date today is not overdue until the day ends.
      item(6, work, undefined, new Date(2026, 2, 10)),
    ];
    expect(keys(group_smart_view_items(items, "due", now))).toEqual([
      ["overdue", ["task_4"]],
      ["today", ["task_3", "task_6"]],
      ["upcoming", ["task_1", "task_5"]],
      ["unscheduled", ["task_2"]],
    ]);
  });
});
