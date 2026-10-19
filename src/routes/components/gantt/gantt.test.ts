import { describe, expect, it } from "vite-plus/test";

import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import {
  build_gantt_rows,
  build_gantt_timeline,
  gantt_header_cells,
  unit_end,
  unit_start,
  visible_row_range,
} from "./gantt";
import { date_from_day_number, day_number } from "../../utils/task-date-range";

function make_task(overrides: Partial<Task> = {}): Task {
  return {
    id: "task_1",
    title: "Task",
    description: "",
    color: "",
    start_time: undefined,
    due_time: undefined,
    labels: [],
    items: [],
    recurrence: undefined,
    ...overrides,
  };
}

function make_column(id: string, tasks: Task[]): Column {
  return { id, name: id, color: "", sort_order: "custom", tasks };
}

const day = (year: number, month: number, date: number) =>
  day_number(new Date(year, month, date));

describe("build_gantt_rows", () => {
  it("groups dated tasks by column in start order and counts undated tasks", () => {
    const late = make_task({
      id: "task_1",
      start_time: new Date(2026, 9, 10),
      due_time: new Date(2026, 9, 12),
    });
    const early = make_task({ id: "task_2", due_time: new Date(2026, 9, 3) });
    const undated = make_task({ id: "task_3" });
    const columns = [
      make_column("todo", [late, undated, early]),
      make_column("empty", [make_task({ id: "task_4" })]),
    ];

    const result = build_gantt_rows(columns, "");

    expect(result.task_count).toBe(2);
    expect(result.undated_count).toBe(2);
    expect(result.rows.map((row) => row.key)).toEqual([
      "column:todo",
      "column:todo:task_2",
      "column:todo:task_1",
    ]);
  });

  it("keeps board order for equal ranges", () => {
    const due_time = new Date(2026, 9, 3);
    const tasks = ["task_3", "task_1", "task_2"].map((id) =>
      make_task({ id, due_time }),
    );

    const result = build_gantt_rows([make_column("todo", tasks)], "");

    expect(
      result.rows.flatMap((row) =>
        row.kind === "task" ? [row.entry.task.id] : [],
      ),
    ).toEqual(["task_3", "task_1", "task_2"]);
  });

  it("filters by search results and appends archived tasks when shown", () => {
    const due_time = new Date(2026, 9, 3);
    const columns = [
      make_column("todo", [
        make_task({ id: "task_1", due_time }),
        make_task({ id: "task_2", due_time }),
      ]),
    ];
    const archives = [
      { time: new Date(), task: make_task({ id: "task_9", due_time }) },
    ];
    const ids = new Set(["task_2", "task_9"]);

    const hidden = build_gantt_rows(columns, "x", archives, false, ids);
    const shown = build_gantt_rows(columns, "x", archives, true, ids);

    expect(hidden.rows.map((row) => row.key)).toEqual([
      "column:todo",
      "column:todo:task_2",
    ]);
    expect(shown.rows.map((row) => row.key)).toEqual([
      "column:todo",
      "column:todo:task_2",
      "archived",
      "archived:task_9",
    ]);
    const archived = shown.rows[3];
    expect(archived.kind === "task" && archived.entry.archived).toBe(true);
    expect(archived.kind === "task" && archived.entry.column).toBeNull();
  });
});

describe("scale units", () => {
  it("aligns weeks to Sunday and months to their first and last days", () => {
    const wednesday = day(2026, 9, 14);

    expect(unit_start(wednesday, "week")).toBe(day(2026, 9, 11));
    expect(unit_end(wednesday, "week")).toBe(day(2026, 9, 17));
    expect(unit_start(wednesday, "month")).toBe(day(2026, 9, 1));
    expect(unit_end(wednesday, "month")).toBe(day(2026, 9, 31));
    expect(unit_start(day(2024, 1, 10), "month")).toBe(day(2024, 1, 1));
    expect(unit_end(day(2024, 1, 10), "month")).toBe(day(2024, 1, 29));
  });
});

describe("build_gantt_timeline", () => {
  it("covers today and every task with padding aligned to whole units", () => {
    const today = day(2026, 9, 18);
    const rows = build_gantt_rows(
      [
        make_column("todo", [
          make_task({
            start_time: new Date(2026, 10, 2),
            due_time: new Date(2026, 11, 20),
          }),
        ]),
      ],
      "",
    ).rows;

    const timeline = build_gantt_timeline(rows, today, "month");

    expect(timeline.first_day).toBe(day(2026, 7, 1));
    expect(timeline.last_day).toBe(day(2027, 1, 28));
    expect(timeline.width).toBe(
      (timeline.last_day - timeline.first_day + 1) * timeline.day_width,
    );
  });

  it("centers an empty board on today", () => {
    const today = day(2026, 9, 18);

    const timeline = build_gantt_timeline([], today, "day");

    expect(timeline.first_day).toBe(today - 14);
    expect(timeline.last_day).toBe(today + 14);
  });

  it("extends the end to fill the viewport width in whole units", () => {
    const today = day(2026, 9, 18);

    const timeline = build_gantt_timeline([], today, "month", 2000);

    expect(timeline.width).toBeGreaterThanOrEqual(2000);
    expect(timeline.last_day).toBe(unit_end(timeline.last_day, "month"));
    expect(timeline.first_day).toBe(day(2026, 7, 1));
  });

  it("adds extension days beyond the padding on both sides", () => {
    const today = day(2026, 9, 18);

    const timeline = build_gantt_timeline([], today, "day", 0, 30, 45);

    expect(timeline.first_day).toBe(today - 14 - 30);
    expect(timeline.last_day).toBe(today + 14 + 45);
  });
});

describe("gantt_header_cells", () => {
  it("splits the timeline into clipped unit cells without gaps", () => {
    const timeline = {
      first_day: day(2026, 9, 25),
      last_day: day(2026, 10, 3),
      day_width: 10,
      width: 100,
    };

    const months = gantt_header_cells(timeline, "month");
    const weeks = gantt_header_cells(timeline, "week");

    expect(months.map((cell) => [cell.x, cell.width])).toEqual([
      [0, 70],
      [70, 30],
    ]);
    expect(date_from_day_number(months[1].day)).toEqual(new Date(2026, 10, 1));
    // 2026-10-25 is a Sunday.
    expect(weeks.map((cell) => [cell.x, cell.width])).toEqual([
      [0, 70],
      [70, 30],
    ]);
    expect(gantt_header_cells(timeline, "day")).toHaveLength(10);
    expect(gantt_header_cells(timeline, "year")).toEqual([
      {
        key: `year:${timeline.first_day}`,
        day: timeline.first_day,
        x: 0,
        width: 100,
      },
    ]);
  });

  it("keeps day cells aligned across a daylight saving change", () => {
    const timeline = {
      first_day: day(2026, 2, 7),
      last_day: day(2026, 2, 10),
      day_width: 1,
      width: 4,
    };

    expect(
      gantt_header_cells(timeline, "day").map((cell) =>
        date_from_day_number(cell.day).getDate(),
      ),
    ).toEqual([7, 8, 9, 10]);
  });
});

describe("visible_row_range", () => {
  it("returns the rows in view with overscan, clamped to the row count", () => {
    expect(visible_row_range(0, 100, 20, 50, 2)).toEqual({ start: 0, end: 7 });
    expect(visible_row_range(400, 100, 20, 50, 2)).toEqual({
      start: 18,
      end: 27,
    });
    expect(visible_row_range(990, 100, 20, 50, 2)).toEqual({
      start: 47,
      end: 50,
    });
    expect(visible_row_range(0, 100, 20, 0)).toEqual({ start: 0, end: 0 });
  });
});
