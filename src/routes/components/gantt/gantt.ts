import type { Column } from "../../type/column.svelte";
import type { Archive } from "../../type/archive.svelte";
import type { Task } from "../../type/task.svelte";
import { task_matches_search } from "../calendar/calendar";
import {
  date_from_day_number,
  day_number,
  task_date_range,
  type TaskDateRange,
} from "../../utils/task-date-range";

export type GanttScale = "day" | "week" | "month";

export interface GanttScaleConfig {
  /** Width of one calendar day in pixels. */
  day_width: number;
  /** Days added before the earliest and after the latest task. */
  padding_days: number;
}

export const GANTT_SCALES: Record<GanttScale, GanttScaleConfig> = {
  day: { day_width: 36, padding_days: 14 },
  week: { day_width: 16, padding_days: 28 },
  month: { day_width: 4, padding_days: 62 },
};

export interface GanttTask {
  task: Task;
  /** Null for archived tasks, which no longer belong to a column. */
  column: Column | null;
  archived: boolean;
  range: TaskDateRange;
}

export type GanttRow =
  | {
      kind: "group";
      key: string;
      column: Column | null;
      count: number;
    }
  | { kind: "task"; key: string; entry: GanttTask };

export interface GanttRows {
  rows: GanttRow[];
  task_count: number;
  /** Matching tasks without a start or due date, which have no bar. */
  undated_count: number;
}

function sort_entries(entries: GanttTask[]) {
  // Array.sort is stable, so equal ranges keep the board order.
  entries.sort(
    (left, right) =>
      left.range.start_day - right.range.start_day ||
      left.range.end_day - right.range.end_day,
  );
}

/**
 * One group row per column that has dated tasks, followed by its tasks in
 * start order. Archived tasks form a final group when shown.
 */
export function build_gantt_rows(
  columns: Column[],
  search_text: string,
  archives: Archive[] = [],
  show_archived = false,
  search_task_ids?: ReadonlySet<string> | null,
): GanttRows {
  const rows: GanttRow[] = [];
  let task_count = 0;
  let undated_count = 0;

  const add_group = (
    key: string,
    column: Column | null,
    tasks: Task[],
    archived: boolean,
  ) => {
    const entries: GanttTask[] = [];
    for (const task of tasks) {
      if (!task_matches_search(task, search_text, search_task_ids)) {
        continue;
      }
      const range = task_date_range(task);
      if (range) {
        entries.push({ task, column, archived, range });
      } else {
        undated_count += 1;
      }
    }
    if (entries.length === 0) {
      return;
    }
    sort_entries(entries);
    task_count += entries.length;
    rows.push({ kind: "group", key, column, count: entries.length });
    for (const entry of entries) {
      rows.push({ kind: "task", key: `${key}:${entry.task.id}`, entry });
    }
  };

  for (const column of columns) {
    add_group(`column:${column.id}`, column, column.tasks, false);
  }
  if (show_archived) {
    add_group(
      "archived",
      null,
      archives.map(({ task }) => task),
      true,
    );
  }
  return { rows, task_count, undated_count };
}

/** First day of the scale unit (Sunday-based week or month) containing a day. */
export function unit_start(day: number, scale: GanttScale): number {
  const date = date_from_day_number(day);
  if (scale === "week") {
    return day - date.getDay();
  }
  if (scale === "month") {
    return day_number(new Date(date.getFullYear(), date.getMonth(), 1));
  }
  return day;
}

/** Last day of the scale unit containing a day. */
export function unit_end(day: number, scale: GanttScale): number {
  const date = date_from_day_number(day);
  if (scale === "week") {
    return day + 6 - date.getDay();
  }
  if (scale === "month") {
    return day_number(new Date(date.getFullYear(), date.getMonth() + 1, 0));
  }
  return day;
}

export interface GanttTimeline {
  first_day: number;
  /** Inclusive. */
  last_day: number;
  day_width: number;
  width: number;
}

/**
 * The padded range covering today and every task, aligned to whole units and
 * extended to at least `min_width` pixels. `extend_before` and `extend_after`
 * add days beyond the padding, for scrolling past the tasks.
 */
export function build_gantt_timeline(
  rows: GanttRow[],
  today_day: number,
  scale: GanttScale,
  min_width = 0,
  extend_before = 0,
  extend_after = 0,
): GanttTimeline {
  let min = today_day;
  let max = today_day;
  for (const row of rows) {
    if (row.kind === "task") {
      min = Math.min(min, row.entry.range.start_day);
      max = Math.max(max, row.entry.range.end_day);
    }
  }
  const { day_width, padding_days } = GANTT_SCALES[scale];
  const first_day = unit_start(min - padding_days - extend_before, scale);
  const min_days = Math.ceil(min_width / day_width);
  const last_day = unit_end(
    Math.max(max + padding_days + extend_after, first_day + min_days - 1),
    scale,
  );
  return {
    first_day,
    last_day,
    day_width,
    width: (last_day - first_day + 1) * day_width,
  };
}

export interface GanttHeaderCell {
  key: string;
  /** First day of the cell. */
  day: number;
  x: number;
  width: number;
}

/**
 * Consecutive cells of one unit across the timeline. The first and last
 * cells are clipped to the timeline.
 */
export function gantt_header_cells(
  timeline: GanttTimeline,
  unit: "day" | "week" | "month" | "year",
): GanttHeaderCell[] {
  const cells: GanttHeaderCell[] = [];
  let day = timeline.first_day;
  while (day <= timeline.last_day) {
    const date = date_from_day_number(day);
    let next: number;
    if (unit === "day") {
      next = day + 1;
    } else if (unit === "week") {
      next = day - date.getDay() + 7;
    } else if (unit === "month") {
      next = day_number(new Date(date.getFullYear(), date.getMonth() + 1, 1));
    } else {
      next = day_number(new Date(date.getFullYear() + 1, 0, 1));
    }
    const end = Math.min(next, timeline.last_day + 1);
    cells.push({
      key: `${unit}:${day}`,
      day,
      x: (day - timeline.first_day) * timeline.day_width,
      width: (end - day) * timeline.day_width,
    });
    day = end;
  }
  return cells;
}

/**
 * The cells intersecting a horizontal window of the timeline. The window is
 * snapped to whole viewport widths with one width of overscan on each side,
 * so the result only changes after scrolling about a screen.
 */
export function visible_cells(
  cells: GanttHeaderCell[],
  scroll_left: number,
  viewport_width: number,
): GanttHeaderCell[] {
  if (viewport_width <= 0) {
    return cells;
  }
  const start = (Math.floor(scroll_left / viewport_width) - 1) * viewport_width;
  const end = start + viewport_width * 4;
  return cells.filter((cell) => cell.x < end && cell.x + cell.width > start);
}

/** Indices of the rows intersecting the viewport, with overscan. */
export function visible_row_range(
  scroll_top: number,
  viewport_height: number,
  row_height: number,
  row_count: number,
  overscan = 8,
): { start: number; end: number } {
  const start = Math.max(0, Math.floor(scroll_top / row_height) - overscan);
  const end = Math.min(
    row_count,
    Math.ceil((scroll_top + viewport_height) / row_height) + overscan,
  );
  return { start, end: Math.max(start, end) };
}
