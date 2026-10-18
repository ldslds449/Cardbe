import { describe, expect, it } from "vite-plus/test";

import {
  date_from_day_number,
  day_number,
  due_limit,
  is_valid_date_range,
  move_task_dates,
  ranges_intersect,
  resize_task_dates,
  shift_days,
  task_date_range,
} from "./task-date-range";

const day = (year: number, month: number, date: number) =>
  day_number(new Date(year, month, date));

describe("task_date_range", () => {
  it("does not place tasks without dates", () => {
    expect(
      task_date_range({ start_time: undefined, due_time: undefined }),
    ).toBeNull();
  });

  it("anchors due-only tasks on the due day", () => {
    expect(
      task_date_range({
        start_time: undefined,
        due_time: new Date(2026, 9, 22, 18),
      }),
    ).toEqual({
      kind: "due",
      start_day: day(2026, 9, 22),
      end_day: day(2026, 9, 22),
      invalid: false,
    });
  });

  it("anchors start-only tasks on the start day without inventing an end", () => {
    expect(
      task_date_range({
        start_time: new Date(2026, 9, 19, 9),
        due_time: undefined,
      }),
    ).toEqual({
      kind: "start",
      start_day: day(2026, 9, 19),
      end_day: day(2026, 9, 19),
      invalid: false,
    });
  });

  it("treats a start and due on the same day as a single-day task", () => {
    expect(
      task_date_range({
        start_time: new Date(2026, 9, 19, 9),
        due_time: new Date(2026, 9, 19, 18),
      })?.kind,
    ).toBe("single");
  });

  it("returns an inclusive span across weeks, months and years", () => {
    expect(
      task_date_range({
        start_time: new Date(2026, 11, 28, 9),
        due_time: new Date(2027, 0, 4, 18),
      }),
    ).toEqual({
      kind: "span",
      start_day: day(2026, 11, 28),
      end_day: day(2027, 0, 4),
      invalid: false,
    });
  });

  it("flags a start after the due time and shows it on the due day", () => {
    const range = task_date_range({
      start_time: new Date(2026, 9, 23, 9),
      due_time: new Date(2026, 9, 22, 18),
    });
    expect(range).toEqual({
      kind: "due",
      start_day: day(2026, 9, 22),
      end_day: day(2026, 9, 22),
      invalid: true,
    });
  });

  it("lets a timed start fall on a date-only due day", () => {
    const due_time = new Date(2026, 9, 22);
    expect(due_limit(due_time)).toBe(new Date(2026, 9, 23).getTime() - 1);
    expect(
      is_valid_date_range({ start_time: new Date(2026, 9, 22, 15), due_time }),
    ).toBe(true);
    expect(
      is_valid_date_range({
        start_time: new Date(2026, 9, 22, 15),
        due_time: new Date(2026, 9, 22, 9),
      }),
    ).toBe(false);
  });
});

describe("day arithmetic", () => {
  it("counts calendar days independently of DST offsets", () => {
    // Daylight saving transitions in the northern and southern hemispheres.
    for (const [year, month, date] of [
      [2026, 2, 8],
      [2026, 10, 1],
      [2026, 2, 29],
      [2026, 3, 5],
      [2026, 9, 4],
    ]) {
      const before = new Date(year, month, date - 1);
      const after = new Date(year, month, date + 1);
      expect(day_number(after) - day_number(before)).toBe(2);
      expect(date_from_day_number(day_number(after))).toEqual(after);
    }
  });

  it("handles dates before the Unix epoch", () => {
    const date = new Date(1969, 11, 31);
    expect(date_from_day_number(day_number(date))).toEqual(date);
  });

  it("shifts dates by calendar days keeping the wall-clock time", () => {
    const time = new Date(2026, 2, 7, 9, 15, 30, 250);
    const shifted = shift_days(time, 2);
    expect(shifted).toEqual(new Date(2026, 2, 9, 9, 15, 30, 250));
    expect(shift_days(new Date(2026, 10, 30, 23, 59), 2)).toEqual(
      new Date(2026, 11, 2, 23, 59),
    );
    expect(shift_days(new Date(2026, 11, 31, 8), 1)).toEqual(
      new Date(2027, 0, 1, 8),
    );
  });

  it("detects overlap with a visible range", () => {
    const visible = [day(2026, 9, 1), day(2026, 9, 31)] as const;
    expect(
      ranges_intersect(
        { start_day: day(2026, 8, 20), end_day: day(2026, 9, 2) },
        ...visible,
      ),
    ).toBe(true);
    expect(
      ranges_intersect(
        { start_day: day(2026, 8, 20), end_day: day(2026, 11, 2) },
        ...visible,
      ),
    ).toBe(true);
    expect(
      ranges_intersect(
        { start_day: day(2026, 8, 20), end_day: day(2026, 8, 30) },
        ...visible,
      ),
    ).toBe(false);
  });
});

describe("move_task_dates", () => {
  it("moves start and due together, keeping the span and times", () => {
    const result = move_task_dates(
      {
        start_time: new Date(2026, 9, 19, 9, 0),
        due_time: new Date(2026, 9, 22, 18, 0),
      },
      2,
    );
    expect(result).toEqual({
      start_time: new Date(2026, 9, 21, 9, 0),
      due_time: new Date(2026, 9, 24, 18, 0),
    });
  });

  it("changes only the existing field of single-date tasks", () => {
    expect(
      move_task_dates(
        { start_time: undefined, due_time: new Date(2026, 9, 22, 18) },
        -3,
      ),
    ).toEqual({ start_time: undefined, due_time: new Date(2026, 9, 19, 18) });
    expect(
      move_task_dates(
        { start_time: new Date(2026, 9, 22, 8), due_time: undefined },
        1,
      ),
    ).toEqual({ start_time: new Date(2026, 9, 23, 8), due_time: undefined });
  });

  it("keeps the legacy due-only move for an invalid range", () => {
    const start_time = new Date(2026, 9, 25, 9);
    expect(
      move_task_dates({ start_time, due_time: new Date(2026, 9, 22, 18) }, 1),
    ).toEqual({ start_time, due_time: new Date(2026, 9, 23, 18) });
  });
});

describe("resize_task_dates", () => {
  const dates = {
    start_time: new Date(2026, 9, 19, 9, 0),
    due_time: new Date(2026, 9, 22, 18, 0),
  };

  it("changes only the date part of the dragged edge", () => {
    expect(resize_task_dates(dates, "start", day(2026, 9, 17))).toEqual({
      start_time: new Date(2026, 9, 17, 9, 0),
      due_time: dates.due_time,
    });
    expect(resize_task_dates(dates, "end", day(2026, 10, 2))).toEqual({
      start_time: dates.start_time,
      due_time: new Date(2026, 10, 2, 18, 0),
    });
  });

  it("allows collapsing to one day when times stay ordered", () => {
    expect(resize_task_dates(dates, "end", day(2026, 9, 19))?.due_time).toEqual(
      new Date(2026, 9, 19, 18, 0),
    );
  });

  it("rejects a due time earlier than the start", () => {
    expect(resize_task_dates(dates, "end", day(2026, 9, 18))).toBeNull();
    expect(resize_task_dates(dates, "start", day(2026, 9, 23))).toBeNull();
    expect(
      resize_task_dates(
        {
          start_time: new Date(2026, 9, 19, 20, 0),
          due_time: new Date(2026, 9, 22, 18, 0),
        },
        "end",
        day(2026, 9, 19),
      ),
    ).toBeNull();
  });

  it("turns a due-only task into a range", () => {
    const due_only = { start_time: undefined, due_time: dates.due_time };
    expect(resize_task_dates(due_only, "start", day(2026, 9, 18))).toEqual({
      start_time: new Date(2026, 9, 18),
      due_time: dates.due_time,
    });
    expect(resize_task_dates(due_only, "end", day(2026, 9, 24))).toEqual({
      start_time: new Date(2026, 9, 22),
      due_time: new Date(2026, 9, 24, 18, 0),
    });
    expect(resize_task_dates(due_only, "start", day(2026, 9, 22))).toBeNull();
    expect(resize_task_dates(due_only, "start", day(2026, 9, 23))).toBeNull();
    expect(resize_task_dates(due_only, "end", day(2026, 9, 21))).toBeNull();
  });

  it("turns a start-only task into a range", () => {
    const start_only = { start_time: dates.start_time, due_time: undefined };
    expect(resize_task_dates(start_only, "end", day(2026, 9, 21))).toEqual({
      start_time: dates.start_time,
      due_time: new Date(2026, 9, 21),
    });
    expect(resize_task_dates(start_only, "start", day(2026, 9, 17))).toEqual({
      start_time: new Date(2026, 9, 17, 9, 0),
      due_time: new Date(2026, 9, 19),
    });
    expect(resize_task_dates(start_only, "end", day(2026, 9, 19))).toBeNull();
  });

  it("does not resize tasks without dates", () => {
    expect(
      resize_task_dates(
        { start_time: undefined, due_time: undefined },
        "end",
        day(2026, 9, 18),
      ),
    ).toBeNull();
  });
});
