import { done_column, type Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import { task_matches_search } from "../calendar/calendar";
import { due_limit } from "../../utils/task-date-range";

export interface TodayTask {
  task: Task;
  column: Column;
}

export interface TodayGroups {
  overdue: TodayTask[];
  today: TodayTask[];
  upcoming: TodayTask[];
  unscheduled: TodayTask[];
}

function sort_by_due_time(left: TodayTask, right: TodayTask): number {
  return (
    (left.task.due_time?.getTime() ?? 0) - (right.task.due_time?.getTime() ?? 0)
  );
}

export function build_today_groups(
  columns: Column[],
  now: Date,
  search_text: string,
  search_task_ids?: ReadonlySet<string> | null,
): TodayGroups {
  const groups: TodayGroups = {
    overdue: [],
    today: [],
    upcoming: [],
    unscheduled: [],
  };
  const tomorrow_start = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1,
  ).getTime();

  const done = done_column(columns);
  for (const column of columns) {
    // Completed tasks need no more attention.
    if (column === done) {
      continue;
    }
    for (const task of column.tasks) {
      if (!task_matches_search(task, search_text, search_task_ids)) {
        continue;
      }

      const entry = { task, column };
      if (!task.due_time) {
        groups.unscheduled.push(entry);
      } else if (
        due_limit(task.due_time, task.due_at_midnight) < now.getTime()
      ) {
        // Past its due time, or past the end of the day for a date-only due.
        groups.overdue.push(entry);
      } else if (task.due_time.getTime() < tomorrow_start) {
        groups.today.push(entry);
      } else {
        groups.upcoming.push(entry);
      }
    }
  }

  groups.overdue.sort(sort_by_due_time);
  groups.today.sort(sort_by_due_time);
  groups.upcoming.sort(sort_by_due_time);
  return groups;
}

export function today_task_count(groups: TodayGroups): number {
  return (
    groups.overdue.length +
    groups.today.length +
    groups.upcoming.length +
    groups.unscheduled.length
  );
}
