import type { Task } from "../type/task.svelte";

// Calendar-day semantics shared by the calendar and future timeline views.
// Days are local calendar dates; ranges are inclusive of both end days.
//
// A time at local midnight is a date without a time of day, unless the task
// marks it as a chosen 00:00 (`start_at_midnight` / `due_at_midnight`). A
// date-only start begins at that midnight; a date-only due date lasts until the
// end of its day. The backend applies the same rules in `task_dates.rs`.

export type TaskDateRangeKind = "due" | "start" | "single" | "span";

export interface TaskDateRange {
  kind: TaskDateRangeKind;
  /** Local day number of the first displayed day. */
  start_day: number;
  /** Local day number of the last displayed day (inclusive). */
  end_day: number;
  /** The start time is later than the due time; displayed on the due day. */
  invalid: boolean;
}

export interface TaskDates {
  start_time: Date | undefined;
  due_time: Date | undefined;
  start_at_midnight?: boolean;
  due_at_midnight?: boolean;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** Local calendar day as a sequential integer, independent of DST offsets. */
export function day_number(date: Date): number {
  return Math.round(
    Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / DAY_MS,
  );
}

/** Local midnight of a day number. */
export function date_from_day_number(day: number): Date {
  const utc = new Date(day * DAY_MS);
  return new Date(utc.getUTCFullYear(), utc.getUTCMonth(), utc.getUTCDate());
}

export function is_date_only(date: Date, at_midnight = false): boolean {
  return (
    !at_midnight &&
    date.getHours() === 0 &&
    date.getMinutes() === 0 &&
    date.getSeconds() === 0 &&
    date.getMilliseconds() === 0
  );
}

/** Latest valid start for a due time; a date-only due date covers its whole day. */
export function due_limit(due_time: Date, at_midnight = false): number {
  if (!is_date_only(due_time, at_midnight)) {
    return due_time.getTime();
  }
  const end = new Date(due_time);
  end.setDate(end.getDate() + 1);
  return end.getTime() - 1;
}

export function is_valid_date_range(dates: TaskDates): boolean {
  return (
    dates.start_time === undefined ||
    dates.due_time === undefined ||
    dates.start_time.getTime() <=
      due_limit(dates.due_time, dates.due_at_midnight)
  );
}

export function task_date_range(dates: TaskDates): TaskDateRange | null {
  const { start_time, due_time } = dates;
  if (!due_time) {
    if (!start_time) {
      return null;
    }
    const day = day_number(start_time);
    return { kind: "start", start_day: day, end_day: day, invalid: false };
  }
  const due_day = day_number(due_time);
  if (!start_time) {
    return {
      kind: "due",
      start_day: due_day,
      end_day: due_day,
      invalid: false,
    };
  }
  if (!is_valid_date_range(dates)) {
    return { kind: "due", start_day: due_day, end_day: due_day, invalid: true };
  }
  const start_day = day_number(start_time);
  return {
    kind: start_day === due_day ? "single" : "span",
    start_day,
    end_day: due_day,
    invalid: false,
  };
}

export function ranges_intersect(
  range: Pick<TaskDateRange, "start_day" | "end_day">,
  first_day: number,
  last_day: number,
): boolean {
  return range.start_day <= last_day && range.end_day >= first_day;
}

/** Moves a time to another local day, keeping its wall-clock time. */
export function with_day(time: Date, day: number): Date {
  const target = date_from_day_number(day);
  return new Date(
    target.getFullYear(),
    target.getMonth(),
    target.getDate(),
    time.getHours(),
    time.getMinutes(),
    time.getSeconds(),
    time.getMilliseconds(),
  );
}

export function shift_days(time: Date, days: number): Date {
  return with_day(time, day_number(time) + days);
}

/**
 * Moves a task by whole calendar days. Only existing fields change; a range
 * whose start is after its due time keeps the legacy due-only move.
 */
export function move_task_dates(dates: TaskDates, days: number): TaskDates {
  const range = task_date_range(dates);
  return {
    ...dates,
    start_time:
      dates.start_time && !range?.invalid
        ? shift_days(dates.start_time, days)
        : dates.start_time,
    due_time: dates.due_time ? shift_days(dates.due_time, days) : undefined,
  };
}

/**
 * Changes the start or due day of a task. Returns null when the change would
 * place the start after the due time.
 *
 * A task with only one time becomes a range: the dragged edge gets a date-only
 * value on the target day and the existing time stays on its own day. Times
 * keep their chosen-midnight flags.
 */
export function resize_task_dates(
  dates: TaskDates,
  edge: "start" | "end",
  day: number,
): TaskDates | null {
  if (!dates.start_time && dates.due_time) {
    const due_day = day_number(dates.due_time);
    if (edge === "start") {
      return day < due_day
        ? {
            ...dates,
            start_time: date_from_day_number(day),
            start_at_midnight: undefined,
          }
        : null;
    }
    return day > due_day
      ? {
          ...dates,
          start_time: date_from_day_number(due_day),
          start_at_midnight: undefined,
          due_time: with_day(dates.due_time, day),
        }
      : null;
  }
  if (dates.start_time && !dates.due_time) {
    const start_day = day_number(dates.start_time);
    if (edge === "end") {
      return day > start_day
        ? {
            ...dates,
            due_time: date_from_day_number(day),
            due_at_midnight: undefined,
          }
        : null;
    }
    return day < start_day
      ? {
          ...dates,
          start_time: with_day(dates.start_time, day),
          due_time: date_from_day_number(start_day),
          due_at_midnight: undefined,
        }
      : null;
  }
  if (!dates.start_time || !dates.due_time) {
    return null;
  }
  const next: TaskDates =
    edge === "start"
      ? { ...dates, start_time: with_day(dates.start_time, day) }
      : { ...dates, due_time: with_day(dates.due_time, day) };
  return is_valid_date_range(next) ? next : null;
}

export function same_task_dates(left: TaskDates, right: TaskDates): boolean {
  return (
    left.start_time?.getTime() === right.start_time?.getTime() &&
    left.due_time?.getTime() === right.due_time?.getTime()
  );
}

export function task_dates(task: Task): TaskDates {
  return {
    start_time: task.start_time,
    due_time: task.due_time,
    start_at_midnight: task.start_at_midnight,
    due_at_midnight: task.due_at_midnight,
  };
}
