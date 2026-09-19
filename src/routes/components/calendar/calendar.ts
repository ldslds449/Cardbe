import type { Column } from "../../type/column.svelte";
import type { Archive } from "../../type/archive.svelte";
import type { Task } from "../../type/task.svelte";

export interface CalendarTask {
  task: Task;
  column: Column;
  archived: boolean;
  preview: boolean;
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

export function task_matches_search(task: Task, search_text: string): boolean {
  const query = search_text.trim().toLowerCase();
  if (!query) return true;

  return (
    task.title.toLowerCase().includes(query) ||
    task.description.toLowerCase().includes(query) ||
    task.labels.some((label) => label.toLowerCase().includes(query)) ||
    task.items.some((item) => item.text.toLowerCase().includes(query))
  );
}

export function count_due_tasks(
  columns: Column[],
  search_text: string,
  archives: Archive[] = [],
  show_archived = false,
): number {
  const active_count = columns.reduce(
    (count, column) =>
      count +
      column.tasks.filter(
        (task) => task.due_time !== undefined && task_matches_search(task, search_text),
      ).length,
    0,
  );

  if (!show_archived) return active_count;
  return (
    active_count +
    archives.filter(
      ({ task }) => task.due_time !== undefined && task_matches_search(task, search_text),
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

function next_recurrence_date(task: Task, from: Date): Date | undefined {
  const recurrence = task.recurrence;
  if (!recurrence) return undefined;
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
    const last_day = new Date(next.getFullYear(), next.getMonth() + 1, 0).getDate();
    next.setDate(Math.min(day, last_day));
  }

  return next;
}

function add_recurring_previews(
  tasks_by_date: Map<string, CalendarTask[]>,
  columns: Column[],
  range_start: Date,
  range_end: Date,
  preview_after: Date,
  search_text: string,
) {
  const first_preview_day = start_of_day(preview_after).getTime();

  for (const column of columns) {
    for (const task of column.tasks) {
      if (!task.due_time || !task.recurrence || !task_matches_search(task, search_text)) continue;

      let occurrence = new Date(task.due_time);
      for (let guard = 0; guard < 100_000; guard++) {
        const next = next_recurrence_date(task, occurrence);
        if (!next || next.getTime() <= occurrence.getTime()) break;
        occurrence = next;
        if (occurrence > range_end) break;
        if (occurrence < range_start || occurrence.getTime() < first_preview_day) continue;

        const preview_task: Task = {
          ...task,
          due_time: new Date(occurrence),
          labels: [...task.labels],
          items: task.items.map((item) => ({ ...item })),
          recurrence: task.recurrence ? { ...task.recurrence } : undefined,
        };
        const key = date_key(occurrence);
        const tasks = tasks_by_date.get(key) ?? [];
        tasks.push({ task: preview_task, column, archived: false, preview: true });
        tasks_by_date.set(key, tasks);
      }
    }
  }
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
): CalendarDay[] {
  const tasks_by_date = new Map<string, CalendarTask[]>();
  const first = view_mode === "week"
    ? start_of_day(visible_month)
    : new Date(visible_month.getFullYear(), visible_month.getMonth(), 1);
  const grid_start = new Date(first);
  grid_start.setDate(first.getDate() - first.getDay());
  const day_count = view_mode === "week" ? 7 : 42;
  const grid_end = new Date(grid_start);
  grid_end.setDate(grid_start.getDate() + day_count - 1);
  grid_end.setHours(23, 59, 59, 999);

  for (const column of columns) {
    for (const task of column.tasks) {
      if (!task.due_time || !task_matches_search(task, search_text)) continue;
      const key = date_key(task.due_time);
      const tasks = tasks_by_date.get(key) ?? [];
      tasks.push({ task, column, archived: false, preview: false });
      tasks_by_date.set(key, tasks);
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
      if (!task.due_time || !task_matches_search(task, search_text)) continue;
      const key = date_key(task.due_time);
      const tasks = tasks_by_date.get(key) ?? [];
      tasks.push({ task, column: archived_column, archived: true, preview: false });
      tasks_by_date.set(key, tasks);
    }
  }

  if (show_recurring_previews) {
    add_recurring_previews(
      tasks_by_date,
      columns,
      grid_start,
      grid_end,
      today,
      search_text,
    );
  }

  for (const tasks of tasks_by_date.values()) {
    tasks.sort(
      (left, right) =>
        (left.task.due_time?.getTime() ?? 0) - (right.task.due_time?.getTime() ?? 0),
    );
  }

  const today_key = date_key(today);

  return Array.from({ length: day_count }, (_, index): CalendarDay => {
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
      tasks: tasks_by_date.get(key) ?? [],
    };
  });
}
