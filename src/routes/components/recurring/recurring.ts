import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import { task_matches_search } from "../calendar/calendar";

export interface RecurringTaskEntry {
  column: Column;
  task: Task;
}

export function get_recurring_tasks(
  columns: Column[],
  search_text = "",
): RecurringTaskEntry[] {
  const query = search_text.trim().toLocaleLowerCase();

  return columns
    .flatMap((column) =>
      column.tasks
        .filter((task) => task.recurrence !== undefined)
        .map((task) => ({ column, task })),
    )
    .filter(
      ({ column, task }) =>
        task_matches_search(task, search_text) ||
        (query.length > 0 && column.name.toLocaleLowerCase().includes(query)),
    )
    .toSorted((a, b) => {
      const a_due = a.task.due_time?.getTime() ?? Number.POSITIVE_INFINITY;
      const b_due = b.task.due_time?.getTime() ?? Number.POSITIVE_INFINITY;
      return a_due - b_due || a.task.title.localeCompare(b.task.title);
    });
}
