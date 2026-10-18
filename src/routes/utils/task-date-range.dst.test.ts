// Vitest runs each test file in its own process, so this time zone only
// applies here. It must be set before any Date is created.
process.env.TZ = "America/New_York";

import { describe, expect, it } from "vite-plus/test";

import {
  date_from_day_number,
  day_number,
  move_task_dates,
  resize_task_dates,
} from "./task-date-range";

const HOUR_MS = 60 * 60 * 1000;

describe("task date ranges across daylight saving changes", () => {
  it("runs in a time zone with daylight saving time", () => {
    expect(new Date(2026, 2, 7).getTimezoneOffset()).not.toBe(
      new Date(2026, 2, 9).getTimezoneOffset(),
    );
  });

  it("counts the 23-hour and 25-hour days as single calendar days", () => {
    expect(
      day_number(new Date(2026, 2, 9)) - day_number(new Date(2026, 2, 8)),
    ).toBe(1);
    expect(
      day_number(new Date(2026, 10, 2)) - day_number(new Date(2026, 10, 1)),
    ).toBe(1);
    expect(date_from_day_number(day_number(new Date(2026, 2, 8, 12)))).toEqual(
      new Date(2026, 2, 8),
    );
  });

  it("keeps wall-clock times when moving over the spring change", () => {
    const start_time = new Date(2026, 2, 7, 9, 0);
    const due_time = new Date(2026, 2, 7, 23, 30);
    const moved = move_task_dates({ start_time, due_time }, 2);

    expect(moved).toEqual({
      start_time: new Date(2026, 2, 9, 9, 0),
      due_time: new Date(2026, 2, 9, 23, 30),
    });
    // The clock skipped an hour, so two calendar days are 47 hours.
    expect(moved.start_time!.getTime() - start_time.getTime()).toBe(
      47 * HOUR_MS,
    );
  });

  it("keeps wall-clock times when resizing over the fall change", () => {
    const start_time = new Date(2026, 9, 30, 0, 0);
    const due_time = new Date(2026, 9, 31, 0, 0);
    const resized = resize_task_dates(
      { start_time, due_time },
      "end",
      day_number(new Date(2026, 10, 3)),
    );

    expect(resized?.due_time).toEqual(new Date(2026, 10, 3, 0, 0));
    expect(resized!.due_time!.getTime() - due_time.getTime()).toBe(
      73 * HOUR_MS,
    );
  });
});
