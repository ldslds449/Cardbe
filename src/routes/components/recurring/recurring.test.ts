import { describe, expect, it } from "vitest";

import type { Column } from "../../type/column.svelte";
import { create_task } from "../../type/task.svelte";
import { get_recurring_tasks } from "./recurring";

function columns(): Column[] {
  const later = create_task(
    "task_1",
    "Weekly review",
    "",
    "",
    undefined,
    new Date("2026-08-20T09:00:00Z"),
  );
  later.recurrence = { frequency: "weekly", interval: 1 };

  const sooner = create_task(
    "task_2",
    "Pay invoice",
    "",
    "",
    undefined,
    new Date("2026-08-05T09:00:00Z"),
  );
  sooner.recurrence = { frequency: "monthly", interval: 1 };

  return [
    { id: "column_1", name: "Work", color: "", sort_order: "custom", tasks: [later, create_task()] },
    { id: "column_2", name: "Finance", color: "", sort_order: "custom", tasks: [sooner] },
  ];
}

describe("recurring task list", () => {
  it("includes only recurring tasks and sorts the next due date first", () => {
    expect(get_recurring_tasks(columns()).map(({ task }) => task.id)).toEqual([
      "task_2",
      "task_1",
    ]);
  });

  it("searches task content and column names", () => {
    expect(get_recurring_tasks(columns(), "finance").map(({ task }) => task.id)).toEqual([
      "task_2",
    ]);
    expect(get_recurring_tasks(columns(), "weekly").map(({ task }) => task.id)).toEqual([
      "task_1",
    ]);
  });
});
