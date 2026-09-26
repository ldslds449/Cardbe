import {
  type Task,
  type TaskSerialized,
  deserialize_task,
} from "./task.svelte";

export type ColumnSort = "custom" | "due_date_asc" | "due_date_desc";

export interface Column {
  id: string;
  name: string;
  color: string;
  sort_order: ColumnSort;
  tasks: Task[];
}

export interface ColumnSerialized {
  id: number;
  name: string;
  color: string;
  sort_order?: ColumnSort;
  tasks: TaskSerialized[];
}

export function create_column(
  id: string = "",
  name: string = "",
  color: string = "",
  tasks: Task[] = [],
  sort_order: ColumnSort = "custom",
): Column {
  return {
    id: id,
    name: name,
    color: color,
    sort_order,
    tasks: tasks,
  };
}

export function set_column_id(column: Column, id: number): void {
  column.id = `column_${id}`;
}

export function get_column_id(column: Column | string): number {
  const value = typeof column === "string" ? column : column.id;
  const match = value.match(/^column_(\d+)$/);
  if (!match) throw new Error(`Invalid column ID: ${value}`);
  return Number.parseInt(match[1], 10);
}

export function deserialize_column(data: ColumnSerialized): Column {
  const column: Column = {
    id: "",
    name: data.name,
    color: data.color,
    sort_order: data.sort_order ?? "custom",
    tasks: data.tasks.map((t) => {
      return deserialize_task(t);
    }),
  };
  set_column_id(column, data.id);
  return column;
}
