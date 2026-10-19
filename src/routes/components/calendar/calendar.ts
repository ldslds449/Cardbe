import type { Column } from "../../type/column.svelte";
import type { Archive } from "../../type/archive.svelte";
import type { Task } from "../../type/task.svelte";
import {
  day_number,
  move_task_dates,
  ranges_intersect,
  resize_task_dates,
  same_task_dates,
  shift_days,
  task_date_range,
  task_dates,
  type TaskDateRange,
  type TaskDates,
} from "../../utils/task-date-range";

export interface CalendarTask {
  task: Task;
  column: Column;
  archived: boolean;
  preview: boolean;
  range: TaskDateRange;
}

export interface CalendarDay {
  date: Date;
  key: string;
  in_current_month: boolean;
  is_today: boolean;
  tasks: CalendarTask[];
}

export function start_of_day(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function date_key(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(
    date.getDate(),
  ).padStart(2, "0")}`;
}

export function task_matches_search(
  task: Task,
  search_text: string,
  search_task_ids?: ReadonlySet<string> | null,
): boolean {
  if (!search_text.trim() || !search_task_ids) {
    return true;
  }
  return search_task_ids.has(task.id);
}

export function task_matches_search_text(
  task: Task,
  search_text: string,
): boolean {
  const query = search_text.trim().toLocaleLowerCase();
  if (!query) {
    return true;
  }
  return [
    task.title,
    task.description,
    ...task.labels,
    ...task.items.map((item) => item.text),
  ].some((text) => text.toLocaleLowerCase().includes(query));
}

export function count_due_tasks(
  columns: Column[],
  search_text: string,
  archives: Archive[] = [],
  show_archived = false,
  search_task_ids?: ReadonlySet<string> | null,
): number {
  const active_count = columns.reduce(
    (count, column) =>
      count +
      column.tasks.filter(
        (task) =>
          task.due_time !== undefined &&
          task_matches_search(task, search_text, search_task_ids),
      ).length,
    0,
  );

  if (!show_archived) {
    return active_count;
  }
  return (
    active_count +
    archives.filter(
      ({ task }) =>
        task.due_time !== undefined &&
        task_matches_search(task, search_text, search_task_ids),
    ).length
  );
}

export type CalendarViewMode = "month" | "week";

export function reschedule_due_time(due_time: Date, target_date: Date): Date {
  return new Date(
    target_date.getFullYear(),
    target_date.getMonth(),
    target_date.getDate(),
    due_time.getHours(),
    due_time.getMinutes(),
    due_time.getSeconds(),
    due_time.getMilliseconds(),
  );
}

export type CalendarDragMode = "move" | "resize-start" | "resize-end";

export interface CalendarDragData {
  task_id: string;
  mode: CalendarDragMode;
  /** The day the pointer grabbed; a move shifts the task by the days from it. */
  grab_day: number;
}

/**
 * The dates a calendar drop produces, or null when nothing changes or the
 * result would place the start after the due time.
 */
export function calendar_drop_dates(
  dates: TaskDates,
  mode: CalendarDragMode,
  grab_day: number,
  target_day: number,
): TaskDates | null {
  const next =
    mode === "move"
      ? move_task_dates(dates, target_day - grab_day)
      : resize_task_dates(
          dates,
          mode === "resize-start" ? "start" : "end",
          target_day,
        );
  return next && !same_task_dates(next, dates) ? next : null;
}

function next_recurrence_date(task: Task, from: Date): Date | undefined {
  const recurrence = task.recurrence;
  if (!recurrence) {
    return undefined;
  }
  const interval = Math.max(1, Math.trunc(recurrence.interval) || 1);
  const next = new Date(from);

  if (recurrence.frequency === "daily") {
    next.setDate(next.getDate() + interval);
  } else if (recurrence.frequency === "weekly") {
    next.setDate(next.getDate() + interval * 7);
  } else {
    const day = next.getDate();
    next.setDate(1);
    next.setMonth(next.getMonth() + interval);
    const last_day = new Date(
      next.getFullYear(),
      next.getMonth() + 1,
      0,
    ).getDate();
    next.setDate(Math.min(day, last_day));
  }

  return next;
}

function recurring_previews(
  task: Task,
  column: Column,
  first_day: number,
  last_day: number,
  preview_after: Date,
): CalendarTask[] {
  const due_time = task.due_time;
  if (!due_time || !task.recurrence) {
    return [];
  }
  const first_preview_day = start_of_day(preview_after).getTime();
  const previews: CalendarTask[] = [];

  let occurrence = new Date(due_time);
  for (let guard = 0; guard < 100_000; guard++) {
    const next = next_recurrence_date(task, occurrence);
    if (!next || next.getTime() <= occurrence.getTime()) {
      break;
    }
    occurrence = next;
    // Each occurrence keeps the task's span, as the backend does when it
    // creates the next recurring task.
    const start_time = task.start_time
      ? shift_days(
          task.start_time,
          day_number(occurrence) - day_number(due_time),
        )
      : undefined;
    const range = task_date_range({
      ...task_dates(task),
      start_time,
      due_time: occurrence,
    });
    if (!range || range.start_day > last_day) {
      break;
    }
    if (range.end_day < first_day || occurrence.getTime() < first_preview_day) {
      continue;
    }

    previews.push({
      task: {
        ...task,
        start_time,
        due_time: new Date(occurrence),
        labels: [...task.labels],
        items: task.items.map((item) => ({ ...item })),
        recurrence: task.recurrence ? { ...task.recurrence } : undefined,
      },
      column,
      archived: false,
      preview: true,
      range,
    });
  }
  return previews;
}

/** Every entry whose displayed date range intersects the visible grid. */
function collect_calendar_entries(
  columns: Column[],
  first_day: number,
  last_day: number,
  today: Date,
  search_text: string,
  archives: Archive[],
  show_archived: boolean,
  show_recurring_previews: boolean,
  search_task_ids?: ReadonlySet<string> | null,
): CalendarTask[] {
  const entries: CalendarTask[] = [];
  const add = (task: Task, column: Column, archived: boolean) => {
    if (!task_matches_search(task, search_text, search_task_ids)) {
      return;
    }
    const range = task_date_range(task);
    if (range && ranges_intersect(range, first_day, last_day)) {
      entries.push({ task, column, archived, preview: false, range });
    }
    if (show_recurring_previews && !archived) {
      entries.push(
        ...recurring_previews(task, column, first_day, last_day, today),
      );
    }
  };

  for (const column of columns) {
    for (const task of column.tasks) {
      add(task, column, false);
    }
  }

  if (show_archived) {
    const archived_column: Column = {
      id: "__archive__",
      name: "Archived",
      color: "",
      sort_order: "custom",
      tasks: [],
    };
    for (const { task } of archives) {
      add(task, archived_column, true);
    }
  }
  return entries;
}

/** The time that orders an entry within its day. */
function entry_anchor_time(entry: CalendarTask): number {
  return (entry.task.due_time ?? entry.task.start_time)?.getTime() ?? 0;
}

function sort_by_anchor_time(entries: CalendarTask[]) {
  entries.sort(
    (left, right) => entry_anchor_time(left) - entry_anchor_time(right),
  );
}

export function calendar_task_key(entry: CalendarTask): string {
  return [
    entry.task.id,
    entry.task.start_time?.getTime() ?? "",
    entry.task.due_time?.getTime() ?? "",
    entry.archived ? "archived" : "active",
    entry.preview ? "preview" : "actual",
  ].join(":");
}

export interface CalendarSpanSegment {
  entry: CalendarTask;
  key: string;
  lane: number;
  /** Zero-based weekday column where this week's segment starts. */
  column: number;
  /** Number of days covered in this week. */
  length: number;
  /** The task starts before this segment (an earlier week or outside the grid). */
  continues_before: boolean;
  /** The task ends after this segment. */
  continues_after: boolean;
}

export interface CalendarSingleEntry {
  entry: CalendarTask;
  key: string;
  lane: number;
}

export interface CalendarWeek {
  key: string;
  days: CalendarDay[];
  /** Visible multi-day bars. */
  segments: CalendarSpanSegment[];
  /** Visible single-day entries per weekday, ordered by time. */
  singles: CalendarSingleEntry[][];
  /** Every entry on each weekday in display order, hidden or not. */
  entries: CalendarTask[][];
  /** Entries per weekday that did not fit; the last lane shows their count. */
  hidden: CalendarTask[][];
  /** Lanes used by visible entries and overflow buttons. */
  lane_count: number;
}

interface PlacedEntry {
  entry: CalendarTask;
  key: string;
  /** Weekday columns covered in this week. */
  start: number;
  end: number;
  lane: number;
}

function compare_keys(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

/**
 * Packs one week's entries into shared lanes: multi-day bars first (earlier,
 * then longer), then single-day entries by time in the remaining gaps. Lanes
 * are assigned per week so later weeks reuse space freed by earlier tasks.
 */
function pack_week(
  entries: CalendarTask[],
  first_day: number,
  last_day: number,
): PlacedEntry[] {
  const spans: PlacedEntry[] = [];
  const singles: PlacedEntry[] = [];
  for (const entry of entries) {
    const { range } = entry;
    const key = calendar_task_key(entry);
    if (range.kind === "span") {
      if (ranges_intersect(range, first_day, last_day)) {
        spans.push({
          entry,
          key,
          start: Math.max(range.start_day, first_day) - first_day,
          end: Math.min(range.end_day, last_day) - first_day,
          lane: 0,
        });
      }
    } else if (range.start_day >= first_day && range.start_day <= last_day) {
      const column = range.start_day - first_day;
      singles.push({ entry, key, start: column, end: column, lane: 0 });
    }
  }
  spans.sort(
    (left, right) =>
      left.start - right.start ||
      right.end - right.start - (left.end - left.start) ||
      left.entry.range.start_day - right.entry.range.start_day ||
      Number(left.entry.preview) - Number(right.entry.preview) ||
      Number(left.entry.archived) - Number(right.entry.archived) ||
      compare_keys(left.key, right.key),
  );
  singles.sort(
    (left, right) =>
      left.start - right.start ||
      entry_anchor_time(left.entry) - entry_anchor_time(right.entry) ||
      compare_keys(left.key, right.key),
  );

  const taken: boolean[][] = Array.from({ length: 7 }, () => []);
  const placed = [...spans, ...singles];
  for (const item of placed) {
    let lane = 0;
    const free = (column: number) => !taken[column][lane];
    while (!range_columns(item).every(free)) {
      lane += 1;
    }
    for (const column of range_columns(item)) {
      taken[column][lane] = true;
    }
    item.lane = lane;
  }
  return placed;
}

function range_columns(item: { start: number; end: number }): number[] {
  return Array.from(
    { length: item.end - item.start + 1 },
    (_, index) => item.start + index,
  );
}

/**
 * Lays out each week with at most `max_lanes` lanes. A day whose entries do not
 * all fit gives its last lane to a "+N" button, so anything there is hidden too;
 * a hidden bar then counts on every day it covers.
 */
export function layout_calendar_weeks(
  days: CalendarDay[],
  entries: CalendarTask[],
  max_lanes: number,
): CalendarWeek[] {
  const lane_limit = Math.max(1, max_lanes);
  const weeks: CalendarWeek[] = [];

  for (let index = 0; index < days.length; index += 7) {
    const week_days = days.slice(index, index + 7);
    const first_day = day_number(week_days[0].date);
    const last_day = first_day + week_days.length - 1;
    const placed = pack_week(entries, first_day, last_day);

    const hidden = new Set<PlacedEntry>();
    const overflow = week_days.map(() => false);
    const hide = (item: PlacedEntry) => {
      hidden.add(item);
      for (const column of range_columns(item)) {
        overflow[column] = true;
      }
    };
    placed.filter((item) => item.lane >= lane_limit).forEach(hide);
    let changed = true;
    while (changed) {
      changed = false;
      for (const item of placed) {
        if (
          item.lane === lane_limit - 1 &&
          !hidden.has(item) &&
          range_columns(item).some((column) => overflow[column])
        ) {
          hide(item);
          changed = true;
        }
      }
    }

    const week: CalendarWeek = {
      key: week_days[0].key,
      days: week_days,
      segments: [],
      singles: week_days.map(() => []),
      entries: week_days.map(() => []),
      hidden: week_days.map(() => []),
      lane_count: overflow.some(Boolean) ? lane_limit : 0,
    };
    const by_lane = [...placed].sort(
      (left, right) => left.lane - right.lane || left.start - right.start,
    );
    for (const item of by_lane) {
      for (const column of range_columns(item)) {
        week.entries[column].push(item.entry);
        if (hidden.has(item)) {
          week.hidden[column].push(item.entry);
        }
      }
      if (hidden.has(item)) {
        continue;
      }
      week.lane_count = Math.max(week.lane_count, item.lane + 1);
      if (item.entry.range.kind === "span") {
        const { range } = item.entry;
        week.segments.push({
          entry: item.entry,
          key: `${item.key}@${week.key}`,
          lane: item.lane,
          column: item.start,
          length: item.end - item.start + 1,
          continues_before: range.start_day < first_day + item.start,
          continues_after: range.end_day > first_day + item.end,
        });
      } else {
        week.singles[item.start].push({
          entry: item.entry,
          key: item.key,
          lane: item.lane,
        });
      }
    }
    weeks.push(week);
  }
  return weeks;
}

export interface CalendarLayout {
  /** Days with entries grouped by due date, as used by summaries and export. */
  days: CalendarDay[];
  weeks: CalendarWeek[];
}

export function build_calendar(
  columns: Column[],
  visible_month: Date,
  today: Date,
  search_text: string,
  archives: Archive[] = [],
  show_archived = false,
  show_recurring_previews = true,
  view_mode: CalendarViewMode = "month",
  search_task_ids?: ReadonlySet<string> | null,
  max_lanes = Number.POSITIVE_INFINITY,
): CalendarLayout {
  const first =
    view_mode === "week"
      ? start_of_day(visible_month)
      : new Date(visible_month.getFullYear(), visible_month.getMonth(), 1);
  const grid_start = new Date(first);
  grid_start.setDate(first.getDate() - first.getDay());
  const day_count = view_mode === "week" ? 7 : 42;
  const first_day = day_number(grid_start);
  const last_day = first_day + day_count - 1;

  const entries = collect_calendar_entries(
    columns,
    first_day,
    last_day,
    today,
    search_text,
    archives,
    show_archived,
    show_recurring_previews,
    search_task_ids,
  );

  const tasks_by_due_day = new Map<number, CalendarTask[]>();
  for (const entry of entries) {
    if (!entry.task.due_time) {
      continue;
    }
    const due_day = day_number(entry.task.due_time);
    const tasks = tasks_by_due_day.get(due_day) ?? [];
    tasks.push(entry);
    tasks_by_due_day.set(due_day, tasks);
  }
  for (const tasks of tasks_by_due_day.values()) {
    sort_by_anchor_time(tasks);
  }

  const today_key = date_key(today);
  const days = Array.from({ length: day_count }, (_, index): CalendarDay => {
    const date = new Date(grid_start);
    date.setDate(grid_start.getDate() + index);
    const key = date_key(date);

    return {
      date,
      key,
      in_current_month:
        view_mode === "week" ||
        (date.getFullYear() === visible_month.getFullYear() &&
          date.getMonth() === visible_month.getMonth()),
      is_today: key === today_key,
      tasks: tasks_by_due_day.get(first_day + index) ?? [],
    };
  });

  return { days, weeks: layout_calendar_weeks(days, entries, max_lanes) };
}

export function build_calendar_days(
  columns: Column[],
  visible_month: Date,
  today: Date,
  search_text: string,
  archives: Archive[] = [],
  show_archived = false,
  show_recurring_previews = true,
  view_mode: CalendarViewMode = "month",
  search_task_ids?: ReadonlySet<string> | null,
): CalendarDay[] {
  return build_calendar(
    columns,
    visible_month,
    today,
    search_text,
    archives,
    show_archived,
    show_recurring_previews,
    view_mode,
    search_task_ids,
  ).days;
}
