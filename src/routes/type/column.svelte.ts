import * as m from "$lib/paraglide/messages.js";
import {
  type Task,
  type TaskSerialized,
  deserialize_task,
} from "./task.svelte";

/** Keep aligned with `SORT_FIELDS` in `src-tauri/src/models.rs`. */
export const SORT_FIELDS = [
  "due_time",
  "start_time",
  "created_time",
  "title",
  "checklist_progress",
  "priority",
] as const;
export const MAX_SORT_RULES = 3;

export type SortField = (typeof SORT_FIELDS)[number];
export type SortDirection = "asc" | "desc";
export const SORT_DIRECTIONS: readonly SortDirection[] = ["asc", "desc"];

export interface SortRule {
  /** A field from a newer version is kept but ignored. */
  field: SortField | (string & {});
  direction: SortDirection;
}

export interface Column {
  id: string;
  name: string;
  color: string;
  /** Display order only; `tasks` keeps the manual order. */
  sort_rules: SortRule[];
  tasks: Task[];
}

export interface ColumnSerialized {
  id: number;
  name: string;
  color: string;
  sort_rules?: SortRule[];
  tasks: TaskSerialized[];
}

export function create_column(
  id: string = "",
  name: string = "",
  color: string = "",
  tasks: Task[] = [],
  sort_rules: SortRule[] = [],
): Column {
  return {
    id: id,
    name: name,
    color: color,
    sort_rules,
    tasks: tasks,
  };
}

export function set_column_id(column: Column, id: number): void {
  column.id = `column_${id}`;
}

export function get_column_id(column: Column | string): number {
  const value = typeof column === "string" ? column : column.id;
  const match = value.match(/^column_(\d+)$/);
  if (!match) {
    throw new Error(m.column_id_invalid({ value }));
  }
  return Number.parseInt(match[1], 10);
}

export function deserialize_column(data: ColumnSerialized): Column {
  const column: Column = {
    id: "",
    name: data.name,
    color: data.color,
    sort_rules: data.sort_rules ?? [],
    tasks: data.tasks.map((t) => {
      return deserialize_task(t);
    }),
  };
  set_column_id(column, data.id);
  return column;
}
