import { describe, expect, it } from "vitest";

import type { Column } from "../../type/column.svelte";
import type { Archive } from "../../type/archive.svelte";
import type { Task } from "../../type/task.svelte";
import {
  build_calendar_days,
  count_due_tasks,
  date_key,
  reschedule_due_time,
  task_matches_search,
} from "./calendar";

describe("reschedule_due_time", () => {
  it("moves a due time to the target local date while preserving its time", () => {
    const due_time = new Date(2026, 0, 31, 14, 25, 36, 420);
    const target = new Date(2026, 2, 8);

    const result = reschedule_due_time(due_time, target);

    expect(result).toEqual(new Date(2026, 2, 8, 14, 25, 36, 420));
    expect(due_time).toEqual(new Date(2026, 0, 31, 14, 25, 36, 420));
  });
});

function make_task(overrides: Partial<Task> = {}): Task {
  return {
    id: "task_1",
    title: "Write release notes",
    description: "Prepare the public changelog",
    color: "",
    start_time: undefined,
    due_time: new Date(2026, 0, 15, 9, 30),
    labels: ["release"],
    items: [{ id: "item_1", text: "Add screenshots", completed: false }],
    recurrence: undefined,
    ...overrides,
  };
}

function make_column(tasks: Task[]): Column {
  return {
    id: "column_1",
    name: "In Progress",
    color: "",
    sort_order: "custom",
    tasks,
  };
}

describe("task_matches_search", () => {
  const task = make_task();

  it("matches title, description, labels, and checklist items case-insensitively", () => {
    expect(task_matches_search(task, "RELEASE NOTES")).toBe(true);
    expect(task_matches_search(task, "public changelog")).toBe(true);
    expect(task_matches_search(task, "release")).toBe(true);
    expect(task_matches_search(task, "screenshots")).toBe(true);
  });

  it("treats blank search text as unfiltered", () => {
    expect(task_matches_search(task, "   ")).toBe(true);
    expect(task_matches_search(task, "missing")).toBe(false);
  });
});

describe("build_calendar_days", () => {
  it("builds only the Sunday-to-Saturday range for week view", () => {
    const days = build_calendar_days(
      [],
      new Date(2026, 7, 5),
      new Date(2026, 7, 5),
      "",
      [],
      false,
      true,
      "week",
    );

    expect(days).toHaveLength(7);
    expect(days[0].key).toBe("2026-08-02");
    expect(days[6].key).toBe("2026-08-08");
    expect(days.every((day) => day.in_current_month)).toBe(true);
  });

  it("builds a six-week Sunday-first grid across month boundaries", () => {
    const days = build_calendar_days(
      [],
      new Date(2026, 0, 1),
      new Date(2026, 0, 15),
      "",
    );

    expect(days).toHaveLength(42);
    expect(days[0].key).toBe("2025-12-28");
    expect(days[41].key).toBe("2026-02-07");
    expect(days.find((day) => day.is_today)?.key).toBe("2026-01-15");
    expect(days.filter((day) => day.in_current_month)).toHaveLength(31);
  });

  it("groups matching tasks by local due date and sorts them by time", () => {
    const later = make_task({ id: "task_2", due_time: new Date(2026, 0, 15, 17, 0) });
    const earlier = make_task({ id: "task_1", due_time: new Date(2026, 0, 15, 8, 0) });
    const filtered = make_task({
      id: "task_3",
      title: "Unrelated task",
      description: "",
      labels: [],
      items: [],
    });

    const days = build_calendar_days(
      [make_column([later, filtered, earlier])],
      new Date(2026, 0, 1),
      new Date(2026, 0, 1),
      "release",
    );
    const due_day = days.find((day) => day.key === date_key(earlier.due_time!));

    expect(due_day?.tasks.map((entry) => entry.task.id)).toEqual(["task_1", "task_2"]);
  });

  it("includes archived tasks only when the archived switch is enabled", () => {
    const archived_task = make_task({ id: "task_9" });
    const archives: Archive[] = [{ time: new Date(2026, 0, 20), task: archived_task }];

    const hidden = build_calendar_days(
      [],
      new Date(2026, 0, 1),
      new Date(2026, 0, 1),
      "",
      archives,
      false,
    );
    const shown = build_calendar_days(
      [],
      new Date(2026, 0, 1),
      new Date(2026, 0, 1),
      "",
      archives,
      true,
    );

    expect(hidden.flatMap((day) => day.tasks)).toHaveLength(0);
    expect(shown.flatMap((day) => day.tasks)).toEqual([
      expect.objectContaining({ task: archived_task, archived: true }),
    ]);
  });

  it("shows future recurring occurrences as virtual previews", () => {
    const task = make_task({
      due_time: new Date(2026, 7, 2, 9, 30),
      recurrence: { frequency: "daily", interval: 2 },
    });
    const days = build_calendar_days(
      [make_column([task])],
      new Date(2026, 7, 1),
      new Date(2026, 7, 1),
      "",
    );

    expect(days.find((day) => day.key === "2026-08-02")?.tasks).toEqual([
      expect.objectContaining({ task, preview: false }),
    ]);
    expect(days.find((day) => day.key === "2026-08-04")?.tasks).toEqual([
      expect.objectContaining({ preview: true }),
    ]);
    expect(days.find((day) => day.key === "2026-08-06")?.tasks).toEqual([
      expect.objectContaining({ preview: true }),
    ]);
  });

  it("does not show missed recurring previews before today", () => {
    const task = make_task({
      due_time: new Date(2026, 7, 1, 9, 30),
      recurrence: { frequency: "daily", interval: 1 },
    });
    const days = build_calendar_days(
      [make_column([task])],
      new Date(2026, 7, 1),
      new Date(2026, 7, 5),
      "",
    );

    expect(days.find((day) => day.key === "2026-08-04")?.tasks).toHaveLength(0);
    expect(days.find((day) => day.key === "2026-08-05")?.tasks).toEqual([
      expect.objectContaining({ preview: true }),
    ]);
  });

  it("clamps monthly previews to the end of shorter months", () => {
    const task = make_task({
      due_time: new Date(2026, 0, 31, 9, 30),
      recurrence: { frequency: "monthly", interval: 1 },
    });
    const days = build_calendar_days(
      [make_column([task])],
      new Date(2026, 1, 1),
      new Date(2026, 0, 1),
      "",
    );

    expect(days.find((day) => day.key === "2026-02-28")?.tasks).toEqual([
      expect.objectContaining({ preview: true }),
    ]);
  });

  it("can hide recurring previews without hiding the real task", () => {
    const task = make_task({ recurrence: { frequency: "weekly", interval: 1 } });
    const days = build_calendar_days(
      [make_column([task])],
      new Date(2026, 0, 1),
      new Date(2026, 0, 1),
      "",
      [],
      false,
      false,
    );

    expect(days.flatMap((day) => day.tasks)).toEqual([
      expect.objectContaining({ task, preview: false }),
    ]);
  });
});

describe("count_due_tasks", () => {
  it("counts only due tasks that match the active search", () => {
    const no_due_time = make_task({ id: "task_2", due_time: undefined });
    const columns = [make_column([make_task(), no_due_time])];

    expect(count_due_tasks(columns, "release")).toBe(1);
    expect(count_due_tasks(columns, "missing")).toBe(0);
  });

  it("counts archived due tasks only when they are visible", () => {
    const archives: Archive[] = [{ time: new Date(), task: make_task({ id: "task_9" }) }];

    expect(count_due_tasks([], "", archives, false)).toBe(0);
    expect(count_due_tasks([], "", archives, true)).toBe(1);
  });
});
