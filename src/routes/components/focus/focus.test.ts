import { describe, expect, it } from "vitest";

import type { Column } from "../../type/column.svelte";
import { create_task } from "../../type/task.svelte";
import { build_focus_groups, focus_task_count } from "./focus";

function make_column(): Column {
  return {
    id: "column_1",
    name: "Todo",
    color: "",
    sort_order: "custom",
    tasks: [],
  };
}

describe("build_focus_groups", () => {
  it("groups tasks by their local due date", () => {
    const column = make_column();
    column.tasks = [
      create_task("task_1", "Yesterday", "", "", undefined, new Date(2026, 6, 24, 23)),
      create_task("task_2", "This morning", "", "", undefined, new Date(2026, 6, 25, 9)),
      create_task("task_3", "Tonight", "", "", undefined, new Date(2026, 6, 25, 20)),
      create_task("task_4", "Tomorrow", "", "", undefined, new Date(2026, 6, 26, 8)),
      create_task("task_5", "Someday"),
    ];

    const groups = build_focus_groups([column], new Date(2026, 6, 25, 16), "");

    expect(groups.overdue.map(({ task }) => task.title)).toEqual(["Yesterday"]);
    expect(groups.today.map(({ task }) => task.title)).toEqual([
      "This morning",
      "Tonight",
    ]);
    expect(groups.upcoming.map(({ task }) => task.title)).toEqual(["Tomorrow"]);
    expect(groups.unscheduled.map(({ task }) => task.title)).toEqual(["Someday"]);
    expect(focus_task_count(groups)).toBe(5);
  });

  it("uses the existing full-text task search", () => {
    const column = make_column();
    column.tasks = [
      create_task(
        "task_1",
        "Write release notes",
        "",
        "",
        undefined,
        new Date(2026, 6, 25),
        ["release"],
      ),
      create_task("task_2", "Buy groceries"),
    ];

    const groups = build_focus_groups([column], new Date(2026, 6, 25), "RELEASE");

    expect(groups.today.map(({ task }) => task.title)).toEqual(["Write release notes"]);
    expect(focus_task_count(groups)).toBe(1);
  });

  it("sorts dated groups by due time", () => {
    const column = make_column();
    column.tasks = [
      create_task("task_1", "Later", "", "", undefined, new Date(2026, 6, 25, 17)),
      create_task("task_2", "Earlier", "", "", undefined, new Date(2026, 6, 25, 9)),
    ];

    const groups = build_focus_groups([column], new Date(2026, 6, 25, 12), "");

    expect(groups.today.map(({ task }) => task.title)).toEqual(["Earlier", "Later"]);
  });
});
