import { describe, expect, it } from "vite-plus/test";

import type { Column } from "../../type/column.svelte";
import type { Archive } from "../../type/archive.svelte";
import type { Task } from "../../type/task.svelte";
import {
  build_calendar,
  build_calendar_days,
  calendar_drop_dates,
  count_due_tasks,
  date_key,
  reschedule_due_time,
  task_matches_search,
  task_matches_search_text,
} from "./calendar";
import { day_number } from "../../utils/task-date-range";

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

  it("filters by backend result IDs", () => {
    expect(task_matches_search(task, "release", new Set(["task_1"]))).toBe(
      true,
    );
    expect(task_matches_search(task, "release", new Set(["task_2"]))).toBe(
      false,
    );
    expect(task_matches_search(task, "   ", new Set())).toBe(true);
  });

  it("matches local panel searches across task fields", () => {
    expect(task_matches_search_text(task, "RELEASE NOTES")).toBe(true);
    expect(task_matches_search_text(task, "public changelog")).toBe(true);
    expect(task_matches_search_text(task, "release")).toBe(true);
    expect(task_matches_search_text(task, "screenshots")).toBe(true);
    expect(task_matches_search_text(task, "missing")).toBe(false);
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
    const later = make_task({
      id: "task_2",
      due_time: new Date(2026, 0, 15, 17, 0),
    });
    const earlier = make_task({
      id: "task_1",
      due_time: new Date(2026, 0, 15, 8, 0),
    });
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
      [],
      false,
      true,
      "month",
      new Set(["task_1", "task_2"]),
    );
    const due_day = days.find((day) => day.key === date_key(earlier.due_time!));

    expect(due_day?.tasks.map((entry) => entry.task.id)).toEqual([
      "task_1",
      "task_2",
    ]);
  });

  it("includes archived tasks only when the archived switch is enabled", () => {
    const archived_task = make_task({ id: "task_9" });
    const archives: Archive[] = [
      { time: new Date(2026, 0, 20), task: archived_task },
    ];

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
    const task = make_task({
      recurrence: { frequency: "weekly", interval: 1 },
    });
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

    expect(
      count_due_tasks(columns, "release", [], false, new Set(["task_1"])),
    ).toBe(1);
    expect(count_due_tasks(columns, "missing", [], false, new Set())).toBe(0);
  });

  it("counts archived due tasks only when they are visible", () => {
    const archives: Archive[] = [
      { time: new Date(), task: make_task({ id: "task_9" }) },
    ];

    expect(count_due_tasks([], "", archives, false)).toBe(0);
    expect(count_due_tasks([], "", archives, true)).toBe(1);
  });
});

describe("build_calendar", () => {
  const span = (id: string, start: Date, due: Date) =>
    make_task({ id, start_time: start, due_time: due });

  function layout(tasks: Task[], visible: Date, max_lanes = 3) {
    return build_calendar(
      [make_column(tasks)],
      visible,
      new Date(2026, 0, 1),
      "",
      [],
      false,
      true,
      "month",
      null,
      max_lanes,
    );
  }

  it("keeps due-only tasks as single-day entries on the due day", () => {
    const task = make_task();
    const { days, weeks } = layout([task], new Date(2026, 0, 1));
    const week = weeks.find((item) =>
      item.days.some((day) => day.key === "2026-01-15"),
    )!;
    const index = week.days.findIndex((day) => day.key === "2026-01-15");

    expect(week.singles[index]).toEqual([
      expect.objectContaining({
        task,
        range: expect.objectContaining({ kind: "due" }),
      }),
    ]);
    expect(week.segments).toHaveLength(0);
    expect(days.find((day) => day.key === "2026-01-15")?.tasks).toHaveLength(1);
  });

  it("places start-only and same-day tasks on their day without a bar", () => {
    const start_only = make_task({
      id: "task_2",
      start_time: new Date(2026, 0, 6, 9),
      due_time: undefined,
    });
    const same_day = span(
      "task_3",
      new Date(2026, 0, 7, 9),
      new Date(2026, 0, 7, 17),
    );
    const { weeks, days } = layout(
      [start_only, same_day],
      new Date(2026, 0, 1),
    );
    const week = weeks[1];

    expect(week.segments).toHaveLength(0);
    expect(week.singles[2].map((entry) => entry.task.id)).toEqual(["task_2"]);
    expect(week.singles[2][0].range.kind).toBe("start");
    expect(week.singles[3].map((entry) => entry.task.id)).toEqual(["task_3"]);
    // Due-date summaries and export list tasks only by their due date.
    expect(
      days.flatMap((day) => day.tasks).map((entry) => entry.task.id),
    ).toEqual(["task_3"]);
  });

  it("shows a task whose start is after its due time on the due day", () => {
    const invalid = span(
      "task_4",
      new Date(2026, 0, 20, 9),
      new Date(2026, 0, 15, 9),
    );
    const { weeks } = layout([invalid], new Date(2026, 0, 1));
    const week = weeks[2];

    expect(week.segments).toHaveLength(0);
    expect(week.singles[4]).toEqual([
      expect.objectContaining({
        range: expect.objectContaining({ kind: "due", invalid: true }),
      }),
    ]);
  });

  it("splits a multi-day task into continuous weekly segments", () => {
    // Thursday, January 8 to Tuesday, January 13, 2026.
    const task = span(
      "task_5",
      new Date(2026, 0, 8, 9),
      new Date(2026, 0, 13, 18),
    );
    const { weeks } = layout([task], new Date(2026, 0, 1));
    const segments = weeks.flatMap((week) =>
      week.segments.map((segment) => ({ week: week.key, ...segment })),
    );

    expect(segments).toEqual([
      expect.objectContaining({
        week: "2026-01-04",
        column: 4,
        length: 3,
        lane: 0,
        continues_before: false,
        continues_after: true,
      }),
      expect.objectContaining({
        week: "2026-01-11",
        column: 0,
        length: 3,
        lane: 0,
        continues_before: true,
        continues_after: false,
      }),
    ]);
    expect(weeks.flatMap((week) => week.singles.flat())).toHaveLength(0);
  });

  it("includes tasks that start before or end after the visible grid", () => {
    // The January 2026 grid runs from December 28, 2025 to February 7, 2026.
    const from_past = span(
      "task_6",
      new Date(2025, 10, 20),
      new Date(2025, 11, 30),
    );
    const into_future = span(
      "task_7",
      new Date(2026, 1, 5),
      new Date(2026, 2, 1),
    );
    const outside = span(
      "task_8",
      new Date(2025, 10, 1),
      new Date(2025, 11, 27),
    );
    const { weeks } = layout(
      [from_past, into_future, outside],
      new Date(2026, 0, 1),
    );

    expect(weeks[0].segments).toEqual([
      expect.objectContaining({
        column: 0,
        length: 3,
        continues_before: true,
        continues_after: false,
      }),
    ]);
    expect(weeks[5].segments).toEqual([
      expect.objectContaining({
        column: 4,
        length: 3,
        continues_before: false,
        continues_after: true,
      }),
    ]);
    expect(
      weeks
        .flatMap((week) => week.segments)
        .map((segment) => segment.entry.task.id),
    ).not.toContain("task_8");
  });

  it("spans across months and years", () => {
    const task = span(
      "task_9",
      new Date(2026, 11, 30, 9),
      new Date(2027, 0, 2, 18),
    );
    const { weeks } = layout([task], new Date(2027, 0, 1));

    expect(weeks[0].key).toBe("2026-12-27");
    expect(weeks[0].segments).toEqual([
      expect.objectContaining({
        column: 3,
        length: 4,
        continues_before: false,
        continues_after: false,
      }),
    ]);
  });

  it("assigns overlapping tasks to separate lanes and reuses free lanes", () => {
    const long = span("task_1", new Date(2026, 0, 5), new Date(2026, 0, 9));
    const overlap = span("task_2", new Date(2026, 0, 6), new Date(2026, 0, 7));
    const later = span("task_3", new Date(2026, 0, 8), new Date(2026, 0, 9));
    const forward = layout([long, overlap, later], new Date(2026, 0, 1))
      .weeks[1];
    const reverse = layout([later, overlap, long], new Date(2026, 0, 1))
      .weeks[1];
    const lanes = (week: typeof forward) =>
      Object.fromEntries(
        week.segments.map((segment) => [segment.entry.task.id, segment.lane]),
      );

    expect(lanes(forward)).toEqual({ task_1: 0, task_2: 1, task_3: 1 });
    expect(lanes(reverse)).toEqual(lanes(forward));
    expect(forward.lane_count).toBe(2);
  });

  it("keeps a cross-week task in one lane and lists hidden overflow", () => {
    const tasks = [
      span("task_1", new Date(2026, 0, 8), new Date(2026, 0, 13)),
      span("task_2", new Date(2026, 0, 9), new Date(2026, 0, 12)),
      span("task_3", new Date(2026, 0, 9), new Date(2026, 0, 10)),
    ];
    const { weeks } = layout(tasks, new Date(2026, 0, 1), 2);
    const lanes = (index: number) =>
      weeks[index].segments.map((segment) => [
        segment.entry.task.id,
        segment.lane,
      ]);

    expect(lanes(1)).toEqual([
      ["task_1", 0],
      ["task_2", 1],
    ]);
    expect(lanes(2)).toEqual(lanes(1));
    expect(weeks[1].hidden[5].map((entry) => entry.task.id)).toEqual([
      "task_3",
    ]);
    expect(weeks[1].hidden[6].map((entry) => entry.task.id)).toEqual([
      "task_3",
    ]);
    expect(weeks[1].lane_count).toBe(2);
  });

  it("keeps the span of recurring previews", () => {
    const task = span(
      "task_1",
      new Date(2026, 7, 3, 9),
      new Date(2026, 7, 4, 18),
    );
    task.recurrence = { frequency: "weekly", interval: 1 };
    const { weeks } = build_calendar(
      [make_column([task])],
      new Date(2026, 7, 1),
      new Date(2026, 7, 1),
      "",
    );

    const previews = weeks
      .flatMap((week) => week.segments)
      .filter((segment) => segment.entry.preview);
    expect(previews[0].entry.task.start_time).toEqual(new Date(2026, 7, 10, 9));
    expect(previews[0].entry.task.due_time).toEqual(new Date(2026, 7, 11, 18));
    expect(previews[0]).toEqual(
      expect.objectContaining({ column: 1, length: 2 }),
    );
  });
});

describe("calendar_drop_dates", () => {
  const dates = {
    start_time: new Date(2026, 9, 19, 9, 0),
    due_time: new Date(2026, 9, 22, 18, 0),
  };
  const day = (date: number) => day_number(new Date(2026, 9, date));

  it("moves a span by the days between the grabbed and target day", () => {
    expect(calendar_drop_dates(dates, "move", day(20), day(22))).toEqual({
      start_time: new Date(2026, 9, 21, 9, 0),
      due_time: new Date(2026, 9, 24, 18, 0),
    });
  });

  it("resizes either edge without changing the other time", () => {
    expect(calendar_drop_dates(dates, "resize-start", 0, day(16))).toEqual({
      start_time: new Date(2026, 9, 16, 9, 0),
      due_time: dates.due_time,
    });
    expect(calendar_drop_dates(dates, "resize-end", 0, day(25))).toEqual({
      start_time: dates.start_time,
      due_time: new Date(2026, 9, 25, 18, 0),
    });
  });

  it("ignores drops that change nothing or end before the start", () => {
    expect(calendar_drop_dates(dates, "move", day(20), day(20))).toBeNull();
    expect(calendar_drop_dates(dates, "resize-end", 0, day(22))).toBeNull();
    expect(calendar_drop_dates(dates, "resize-end", 0, day(18))).toBeNull();
  });

  it("moves single-date tasks without creating the missing field", () => {
    expect(
      calendar_drop_dates(
        { start_time: undefined, due_time: dates.due_time },
        "move",
        day(22),
        day(25),
      ),
    ).toEqual({
      start_time: undefined,
      due_time: new Date(2026, 9, 25, 18, 0),
    });
    expect(
      calendar_drop_dates(
        { start_time: dates.start_time, due_time: undefined },
        "move",
        day(19),
        day(17),
      ),
    ).toEqual({ start_time: new Date(2026, 9, 17, 9, 0), due_time: undefined });
  });
});
