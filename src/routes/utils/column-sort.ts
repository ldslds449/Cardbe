import { m } from "#lib/paraglide/messages.js";
import "#lib/i18n/locale.svelte.js";
import {
  SORT_FIELDS,
  done_column,
  type Column,
  type SortDirection,
  type SortField,
  type SortRule,
} from "../type/column.svelte";
import { TASK_PRIORITIES, type Task } from "../type/task.svelte";
import { due_limit } from "./task-date-range";

// Sort rules only change how a column is displayed. `column.tasks` keeps the
// manual order, which also breaks ties, so switching back restores it.

type SortKey = number | string;

/**
 * The longest chain of open tasks each task of the board waits on, so a task
 * always ranks after the tasks it waits on, even in other columns. Like
 * `open_blockers`, only tasks outside the done column block.
 */
function wait_depths(columns: Column[]): Map<string, number> {
  const done = done_column(columns);
  const open = new Map(
    columns
      .filter((column) => column !== done)
      .flatMap((column) => column.tasks)
      .map((task) => [task.id, task]),
  );
  const depths = new Map<string, number>();
  const visiting = new Set<string>();
  function depth(task: Task): number {
    const known = depths.get(task.id);
    if (known !== undefined) {
      return known;
    }
    // Edits merged from other devices can form a cycle; it is cut here.
    visiting.add(task.id);
    let result = 0;
    for (const id of task.blocked_by ?? []) {
      const blocker = open.get(id);
      if (blocker && !visiting.has(id)) {
        result = Math.max(result, depth(blocker) + 1);
      }
    }
    visiting.delete(task.id);
    depths.set(task.id, result);
    return result;
  }
  for (const task of columns.flatMap((column) => column.tasks)) {
    depth(task);
  }
  return depths;
}

/** Undefined is an empty value; empty values follow all others. */
function sort_key(
  task: Task,
  field: SortField,
  depths: Map<string, number>,
): SortKey | undefined {
  switch (field) {
    case "due_time":
      // A date-only due date lasts until the end of its day.
      return task.due_time
        ? due_limit(task.due_time, task.due_at_midnight)
        : undefined;
    case "start_time":
      return task.start_time?.getTime();
    case "created_time":
      return task.created_time?.getTime() || undefined;
    case "title":
      return task.title.toLowerCase() || undefined;
    case "checklist_progress":
      return task.items.length > 0
        ? task.items.filter((item) => item.completed).length / task.items.length
        : undefined;
    case "priority":
      return task.priority
        ? TASK_PRIORITIES.indexOf(task.priority) + 1
        : undefined;
    case "dependency":
      return depths.get(task.id) ?? 0;
  }
}

/** Compares by code point, unlike `<` on UTF-16 code units. */
function compare_strings(left: string, right: string): number {
  const left_points = Array.from(left, (char) => char.codePointAt(0) ?? 0);
  const right_points = Array.from(right, (char) => char.codePointAt(0) ?? 0);
  const length = Math.min(left_points.length, right_points.length);
  for (let index = 0; index < length; index++) {
    if (left_points[index] !== right_points[index]) {
      return left_points[index] - right_points[index];
    }
  }
  return left_points.length - right_points.length;
}

function compare_keys(left: SortKey, right: SortKey): number {
  return typeof left === "string" && typeof right === "string"
    ? compare_strings(left, right)
    : Number(left) - Number(right);
}

export function is_sort_field(field: string): field is SortField {
  return (SORT_FIELDS as readonly string[]).includes(field);
}

/** Rules this version understands, in priority order. */
export function active_sort_rules(
  rules: SortRule[],
): Array<SortRule & { field: SortField }> {
  return rules.filter((rule): rule is SortRule & { field: SortField } =>
    is_sort_field(rule.field),
  );
}

function compare_tasks(
  left: Task,
  right: Task,
  rules: SortRule[],
  depths: Map<string, number>,
): number {
  const pinned = Number(right.pinned ?? false) - Number(left.pinned ?? false);
  if (pinned) {
    return pinned;
  }
  for (const rule of rules) {
    if (!is_sort_field(rule.field)) {
      continue;
    }
    const left_key = sort_key(left, rule.field, depths);
    const right_key = sort_key(right, rule.field, depths);
    if (left_key === undefined || right_key === undefined) {
      if (left_key !== right_key) {
        return left_key === undefined ? 1 : -1;
      }
      continue;
    }
    const order = compare_keys(left_key, right_key);
    if (order) {
      return rule.direction === "desc" ? -order : order;
    }
  }
  return 0;
}

/**
 * The column's tasks in display order. `columns` is the whole board, which
 * dependency sorting follows across columns.
 */
export function sorted_column_tasks(column: Column, columns: Column[]): Task[] {
  const rules = active_sort_rules(column.sort_rules);
  if (rules.length === 0) {
    return column.tasks;
  }
  const depths = rules.some((rule) => rule.field === "dependency")
    ? wait_depths(columns)
    : new Map<string, number>();
  // `sort` is stable, so ties keep the manual order.
  return [...column.tasks].sort((left, right) =>
    compare_tasks(left, right, rules, depths),
  );
}

export const DEFAULT_SORT_DIRECTION: Record<SortField, SortDirection> = {
  due_time: "asc",
  start_time: "asc",
  created_time: "desc",
  title: "asc",
  checklist_progress: "asc",
  priority: "desc",
  dependency: "asc",
};

export function sort_field_label(field: SortField): string {
  switch (field) {
    case "due_time":
      return m.column_sort_field_due_time();
    case "start_time":
      return m.column_sort_field_start_time();
    case "created_time":
      return m.column_sort_field_created_time();
    case "title":
      return m.column_sort_field_title();
    case "checklist_progress":
      return m.column_sort_field_checklist_progress();
    case "priority":
      return m.task_priority();
    case "dependency":
      return m.column_sort_field_dependency();
  }
}

export function sort_direction_label(
  field: SortField,
  direction: SortDirection,
): string {
  const asc = direction === "asc";
  switch (field) {
    case "due_time":
      return asc ? m.column_sort_due_time_asc() : m.column_sort_due_time_desc();
    case "start_time":
      return asc
        ? m.column_sort_start_time_asc()
        : m.column_sort_start_time_desc();
    case "created_time":
      return asc
        ? m.column_sort_created_time_asc()
        : m.column_sort_created_time_desc();
    case "title":
      return asc ? m.column_sort_title_asc() : m.column_sort_title_desc();
    case "checklist_progress":
      return asc
        ? m.column_sort_checklist_progress_asc()
        : m.column_sort_checklist_progress_desc();
    case "priority":
      return asc ? m.column_sort_priority_asc() : m.column_sort_priority_desc();
    case "dependency":
      return asc
        ? m.column_sort_dependency_asc()
        : m.column_sort_dependency_desc();
  }
}

function sort_rule_label(rule: SortRule & { field: SortField }): string {
  return m.column_sort_rule({
    field: sort_field_label(rule.field),
    direction: sort_direction_label(rule.field, rule.direction),
  });
}

/** Describes active rules; undefined when the column is in manual order. */
export function sort_rules_hint(rules: SortRule[]): string | undefined {
  const [first, second, third] = active_sort_rules(rules).map(sort_rule_label);
  if (third !== undefined) {
    return m.column_sort_hint_three({ first, second, third });
  }
  if (second !== undefined) {
    return m.column_sort_hint_two({ first, second });
  }
  return first === undefined ? undefined : m.column_sort_hint_one({ first });
}
