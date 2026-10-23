import { done_column, type Column } from "../type/column.svelte";
import type { Task } from "../type/task.svelte";

/** Matches `MAX_BLOCKERS` in `src-tauri/src/blocking.rs`. */
export const MAX_BLOCKERS = 50;

/**
 * Open tasks of the board that `task` waits on. Tasks that are done, archived,
 * deleted or moved to another board no longer block it.
 */
export function open_blockers(task: Task, columns: Column[]): Task[] {
  const waits_on = new Set(task.blocked_by ?? []);
  if (waits_on.size === 0) {
    return [];
  }
  const done = done_column(columns);
  return columns
    .filter((column) => column !== done)
    .flatMap((column) => column.tasks)
    .filter((candidate) => waits_on.has(candidate.id));
}

/** Tasks of the board that wait on `task`, whether or not it is open. */
export function waiting_tasks(task: Task, columns: Column[]): Task[] {
  return columns
    .flatMap((column) => column.tasks)
    .filter((candidate) => candidate.blocked_by?.includes(task.id));
}

/**
 * Whether `task_id` may wait on `candidate_id`: not itself, not already
 * chosen, and not a task that waits on `task_id`, directly or through others.
 */
export function can_wait_on(
  task_id: string,
  blocked_by: string[],
  candidate_id: string,
  columns: Column[],
): boolean {
  if (candidate_id === task_id || blocked_by.includes(candidate_id)) {
    return false;
  }
  if (!task_id) {
    return true;
  }
  const waits = new Map(
    columns
      .flatMap((column) => column.tasks)
      .map((task) => [task.id, task.blocked_by ?? []]),
  );
  const visited = new Set<string>();
  const pending = [candidate_id];
  while (pending.length > 0) {
    const id = pending.pop()!;
    if (id === task_id) {
      return false;
    }
    if (!visited.has(id)) {
      visited.add(id);
      pending.push(...(waits.get(id) ?? []));
    }
  }
  return true;
}
