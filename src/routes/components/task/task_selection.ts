import type { Column } from "../../type/column.svelte";

export interface TaskSelection {
  column_ids: string[];
  task_ids: string[];
}

export function filter_selection_columns(
  columns: Column[],
  query: string,
): Column[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) {
    return columns;
  }
  return columns
    .map((column) =>
      column.name.toLocaleLowerCase().includes(needle)
        ? column
        : {
            ...column,
            tasks: column.tasks.filter((task) =>
              [task.title, task.description, ...task.labels]
                .join(" ")
                .toLocaleLowerCase()
                .includes(needle),
            ),
          },
    )
    .filter((column) => column.tasks.length > 0);
}

export function select_column_tasks(
  columns: Column[],
  selection: TaskSelection,
  column_id: string,
  checked: boolean,
): TaskSelection {
  const ids =
    columns
      .find((column) => column.id === column_id)
      ?.tasks.map((task) => task.id) ?? [];
  return {
    column_ids: checked
      ? [...new Set([...selection.column_ids, column_id])]
      : selection.column_ids.filter((id) => id !== column_id),
    task_ids: checked
      ? [...new Set([...selection.task_ids, ...ids])]
      : selection.task_ids.filter((id) => !ids.includes(id)),
  };
}

export function select_one_task(
  selection: TaskSelection,
  column_id: string,
  task_id: string,
  checked: boolean,
): TaskSelection {
  return {
    column_ids: checked
      ? [...new Set([...selection.column_ids, column_id])]
      : selection.column_ids,
    task_ids: checked
      ? [...new Set([...selection.task_ids, task_id])]
      : selection.task_ids.filter((id) => id !== task_id),
  };
}

export function select_visible_tasks(
  columns: Column[],
  visible: Column[],
  selection: TaskSelection,
  checked: boolean,
): TaskSelection {
  const task_ids = new Set(
    visible.flatMap((column) => column.tasks.map((task) => task.id)),
  );
  const column_ids = new Set(
    visible
      .filter((column) => column.tasks.length > 0)
      .map((column) => column.id),
  );
  if (checked) {
    return {
      column_ids: [...new Set([...selection.column_ids, ...column_ids])],
      task_ids: [...new Set([...selection.task_ids, ...task_ids])],
    };
  }
  const remaining = selection.task_ids.filter((id) => !task_ids.has(id));
  return {
    task_ids: remaining,
    column_ids: selection.column_ids.filter(
      (id) =>
        !column_ids.has(id) ||
        (columns
          .find((column) => column.id === id)
          ?.tasks.some((task) => remaining.includes(task.id)) ??
          false),
    ),
  };
}
