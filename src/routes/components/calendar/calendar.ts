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

export interface CalendarWeek {
  key: string;
  days: CalendarDay[];
  segments: CalendarSpanSegment[];
  /** Lanes rendered above the single-day lists of this week. */
  lane_count: number;
  /** Single-day entries per weekday, ordered by time. */
  singles: CalendarTask[][];
  /** Multi-day entries per weekday that did not fit in the visible lanes. */
  hidden: CalendarTask[][];
}

/**
 * Assigns each multi-day entry one lane for the whole grid so a task keeps its
 * row across weeks. Ordering by the real start day, not the clipped one, keeps
 * lanes stable when the visible range changes.
 */
export function assign_span_lanes(
  spans: CalendarTask[],
): Map<CalendarTask, number> {
  const sorted = spans
    .map((entry) => ({ entry, key: calendar_task_key(entry) }))
    .sort(
      (left, right) =>
        left.entry.range.start_day - right.entry.range.start_day ||
        right.entry.range.end_day - left.entry.range.end_day ||
        Number(left.entry.preview) - Number(right.entry.preview) ||
        Number(left.entry.archived) - Number(right.entry.archived) ||
        (left.key < right.key ? -1 : left.key > right.key ? 1 : 0),
    );
  const lane_ends: number[] = [];
  const lanes = new Map<CalendarTask, number>();
  for (const { entry } of sorted) {
    let lane = lane_ends.findIndex((end) => end < entry.range.start_day);
    if (lane === -1) {
      lane = lane_ends.length;
    }
    lane_ends[lane] = entry.range.end_day;
    lanes.set(entry, lane);
  }
  return lanes;
}

export function layout_calendar_weeks(
  days: CalendarDay[],
  entries: CalendarTask[],
  max_lanes: number,
): CalendarWeek[] {
  const spans = entries.filter((entry) => entry.range.kind === "span");
  const lanes = assign_span_lanes(spans);
  const weeks: CalendarWeek[] = [];

  for (let index = 0; index < days.length; index += 7) {
    const week_days = days.slice(index, index + 7);
    const first_day = day_number(week_days[0].date);
    const last_day = first_day + week_days.length - 1;
    const week: CalendarWeek = {
      key: week_days[0].key,
      days: week_days,
      segments: [],
      lane_count: 0,
      singles: week_days.map(() => []),
      hidden: week_days.map(() => []),
    };

    for (const entry of spans) {
      const { range } = entry;
      if (!ranges_intersect(range, first_day, last_day)) {
        continue;
      }
      const start = Math.max(range.start_day, first_day);
      const end = Math.min(range.end_day, last_day);
      const lane = lanes.get(entry) ?? 0;
      if (lane >= max_lanes) {
        for (let day = start; day <= end; day++) {
          week.hidden[day - first_day].push(entry);
        }
        continue;
      }
      week.lane_count = Math.max(week.lane_count, lane + 1);
      week.segments.push({
        entry,
        key: `${calendar_task_key(entry)}@${week.key}`,
        lane,
        column: start - first_day,
        length: end - start + 1,
        continues_before: range.start_day < start,
        continues_after: range.end_day > end,
      });
    }

    for (const entry of entries) {
      const { range } = entry;
      if (
        range.kind !== "span" &&
        range.start_day >= first_day &&
        range.start_day <= last_day
      ) {
        week.singles[range.start_day - first_day].push(entry);
      }
    }
    week.singles.forEach(sort_by_anchor_time);
    week.hidden.forEach(sort_by_anchor_time);
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
