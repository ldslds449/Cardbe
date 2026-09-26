import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import { start_of_day, task_matches_search } from "../calendar/calendar";

export interface FocusTask {
  task: Task;
  column: Column;
}

export interface FocusGroups {
  overdue: FocusTask[];
  today: FocusTask[];
  upcoming: FocusTask[];
  unscheduled: FocusTask[];
}

function sort_by_due_time(left: FocusTask, right: FocusTask): number {
  return (
    (left.task.due_time?.getTime() ?? 0) - (right.task.due_time?.getTime() ?? 0)
  );
}

export function build_focus_groups(
  columns: Column[],
  now: Date,
  search_text: string,
): FocusGroups {
  const groups: FocusGroups = {
    overdue: [],
    today: [],
    upcoming: [],
    unscheduled: [],
  };
  const today_start = start_of_day(now).getTime();
  const tomorrow_start = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1,
  ).getTime();

  for (const column of columns) {
    for (const task of column.tasks) {
      if (!task_matches_search(task, search_text)) continue;

      const due_time = task.due_time?.getTime();
      const entry = { task, column };
      if (due_time === undefined) {
        groups.unscheduled.push(entry);
      } else if (due_time < today_start) {
        groups.overdue.push(entry);
      } else if (due_time < tomorrow_start) {
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

export function focus_task_count(groups: FocusGroups): number {
  return (
    groups.overdue.length +
    groups.today.length +
    groups.upcoming.length +
    groups.unscheduled.length
  );
}
