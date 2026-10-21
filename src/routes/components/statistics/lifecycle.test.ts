import { describe, expect, it } from "vite-plus/test";
import { create_task, type Task } from "../../type/task.svelte";
import { create_column, type Column } from "../../type/column.svelte";
import {
  lifecycle_labels,
  median,
  percentile,
  task_lifecycle,
} from "./lifecycle";

const DAY = 24 * 60 * 60 * 1000;
const now = new Date(2024, 5, 30, 12);

function task(id: string, fields: Partial<Task> = {}): Task {
  return { ...create_task(), id, title: id, ...fields };
}
function ago(days: number) {
  return new Date(now.getTime() - days * DAY);
}
function column(tasks: Task[], name = "Doing"): Column {
  return create_column(name, name, "", tasks);
}

describe("task lifecycle", () => {
  it("measures completion, lead time, due dates and the previous period", () => {
    const archives = [
      { time: ago(1), task: task("a", { created_time: ago(3) }) },
      {
        time: ago(2),
        task: task("b", { created_time: ago(2.5), due_time: ago(3) }),
      },
      // A date-only due date covers its whole day.
      {
        time: ago(5),
        task: task("c", {
          created_time: ago(20),
          due_time: new Date(2024, 5, 25),
        }),
      },
      { time: ago(6), task: task("legacy") },
      { time: ago(10), task: task("previous", { created_time: ago(11) }) },
      { time: ago(20), task: task("old", { created_time: ago(21) }) },
    ];
    const result = task_lifecycle(archives, [], { range: 7, label: null }, now);
    expect(result.completed).toBe(4);
    expect(result.previous_completed).toBe(1);
    expect(result.unknown_created).toBe(1);
    expect(result.lead_time_count).toBe(3);
    expect(result.lead_time_median).toBe(2 * DAY);
    expect(result.lead_time_p85).toBe(15 * DAY);
    expect(result.distribution).toEqual([1, 1, 0, 0, 1, 0]);
    expect(result.due_completed).toBe(2);
    expect(result.on_time).toBe(1);

    const all = task_lifecycle(
      archives,
      [],
      { range: "all", label: null },
      now,
    );
    expect(all.completed).toBe(6);
    expect(all.previous_completed).toBeNull();
    expect(all.weekly).toHaveLength(52);
  });

  it("ignores future archives and creation times after the archive", () => {
    const result = task_lifecycle(
      [
        { time: new Date(now.getTime() + DAY), task: task("future") },
        { time: ago(1), task: task("skewed", { created_time: ago(0) }) },
      ],
      [],
      { range: 30, label: null },
      now,
    );
    expect(result.completed).toBe(1);
    expect(result.lead_time_median).toBeNull();
    expect(result.unknown_created).toBe(1);
  });

  it("orders aging tasks and counts overdue ones without creation times", () => {
    const columns = [
      column([
        task("new", { created_time: ago(1) }),
        task("stale", { created_time: ago(40), due_time: ago(2) }),
        task("unknown", { due_time: ago(1) }),
      ]),
      column(
        [
          task("today", {
            created_time: ago(5),
            due_time: new Date(2024, 5, 30),
          }),
        ],
        "Todo",
      ),
    ];
    const result = task_lifecycle([], columns, { range: 30, label: null }, now);
    expect(result.aging.map((item) => item.task.id)).toEqual([
      "stale",
      "today",
      "new",
    ]);
    expect(result.aging[0]).toMatchObject({
      column_name: "Doing",
      age: 40 * DAY,
      overdue: 2 * DAY,
    });
    expect(result.aging[1].overdue).toBeNull();
    expect(result.overdue).toBe(2);
  });

  it("filters by label and groups lead times per label", () => {
    const archives = [
      {
        time: ago(1),
        task: task("a", { created_time: ago(2), labels: ["work", "home"] }),
      },
      {
        time: ago(1),
        task: task("b", { created_time: ago(4), labels: ["work"] }),
      },
      { time: ago(1), task: task("c", { created_time: ago(3) }) },
    ];
    const columns = [
      column([task("d", { created_time: ago(1), labels: ["home"] })]),
    ];
    const all = task_lifecycle(
      archives,
      columns,
      { range: 30, label: null },
      now,
    );
    expect(all.by_label).toEqual([
      { label: "work", completed: 2, median: 2 * DAY },
      { label: "", completed: 1, median: 2 * DAY },
      { label: "home", completed: 1, median: DAY },
    ]);
    const work = task_lifecycle(
      archives,
      columns,
      { range: 30, label: "work" },
      now,
    );
    expect(work.completed).toBe(2);
    expect(work.aging).toHaveLength(0);
    const unlabeled = task_lifecycle(
      archives,
      columns,
      { range: 30, label: "" },
      now,
    );
    expect(unlabeled.completed).toBe(1);
    expect(lifecycle_labels(archives, columns)).toEqual(["home", "work"]);
  });

  it("buckets created and completed tasks into calendar weeks", () => {
    const result = task_lifecycle(
      [{ time: ago(1), task: task("a", { created_time: ago(8) }) }],
      [column([task("b", { created_time: ago(0) })])],
      { range: 30, label: null },
      now,
    );
    expect(result.weekly).toHaveLength(12);
    // `now` is a Sunday, so the archive on Saturday belongs to the prior week.
    expect(result.weekly[11]).toMatchObject({
      start: new Date(2024, 5, 30),
      created: 1,
      completed: 0,
    });
    expect(result.weekly[10]).toMatchObject({ created: 0, completed: 1 });
    expect(result.weekly[9]).toMatchObject({ created: 1, completed: 0 });
    expect(result.weekly.every((week) => week.start.getDay() === 0)).toBe(true);
  });
});

it("computes median and nearest-rank percentiles", () => {
  expect(median([])).toBeNull();
  expect(median([1, 3])).toBe(2);
  expect(median([1, 2, 9])).toBe(2);
  expect(percentile([], 0.85)).toBeNull();
  expect(percentile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.85)).toBe(9);
});
