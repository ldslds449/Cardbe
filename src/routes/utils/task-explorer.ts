import { due_limit } from "./task-date-range";
import type { TaskPriority } from "../type/task.svelte";

export type PriorityFilter = TaskPriority | "none";

/** Open tasks, tasks in the done column, and archived tasks. */
export type CompletionFilter = "open" | "done" | "archived";

export type WaitingFilter = "waiting" | "not_waiting" | "waited_on";

export interface SearchOptions {
  candidate_limit?: number;
  result_limit?: number;
  rrf_k?: number;
}

export interface TaskExplorerQuery {
  query: string;
  board_ids: number[] | null;
  column_id: number | null;
  status: "all" | "active" | "overdue" | "recurring" | "archived";
  sort:
    | "due"
    | "title"
    | "column"
    | "archived"
    | "priority"
    | "created"
    /** Fused search order; without a query it falls back to title. */
    | "relevance";
  due: "all" | "due" | "none";
  due_start: number | null;
  due_end: number | null;
  archive_start: number | null;
  archive_end: number | null;
  now: number;
  /** Priorities to keep, with `none` for tasks without one; null keeps all. */
  priorities?: PriorityFilter[] | null;
  /** Applies on top of `status`; null keeps all. */
  completion?: CompletionFilter[] | null;
  /** Keeps tasks with any of these labels; null keeps all. */
  labels?: string[] | null;
  waiting?: WaitingFilter | null;
  /** Keeps only recurring tasks, or only the others; null keeps all. */
  recurring?: boolean | null;
  search_options?: SearchOptions;
}

export function date_range_bounds(
  start: string,
  end: string,
): [number | null, number | null] {
  const after_end = end ? new Date(`${end}T00:00:00`) : null;
  after_end?.setDate(after_end.getDate() + 1);
  return [
    start ? new Date(`${start}T00:00:00`).getTime() : null,
    after_end?.getTime() ?? null,
  ];
}

export function is_overdue(
  task: { due_time?: Date; due_at_midnight?: boolean },
  now: Date,
  archived_at?: Date,
): boolean {
  return (
    !archived_at &&
    Boolean(
      task.due_time &&
      due_limit(task.due_time, task.due_at_midnight) < now.getTime(),
    )
  );
}

/**
 * Splits `text` around case-insensitive literal occurrences of `query`.
 * Fuzzy title matches without a literal occurrence stay unmarked.
 */
export function highlight_parts(
  text: string,
  query: string,
): { text: string; match: boolean }[] {
  const needle = query.trim();
  if (!needle) {
    return [{ text, match: false }];
  }
  const pattern = new RegExp(
    needle.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
    "giu",
  );
  const parts: { text: string; match: boolean }[] = [];
  let last = 0;
  for (const found of text.matchAll(pattern)) {
    if (found.index > last) {
      parts.push({ text: text.slice(last, found.index), match: false });
    }
    parts.push({ text: found[0], match: true });
    last = found.index + found[0].length;
  }
  if (last < text.length || parts.length === 0) {
    parts.push({ text: text.slice(last), match: false });
  }
  return parts;
}
