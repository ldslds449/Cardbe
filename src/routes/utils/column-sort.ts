import { m } from "$lib/paraglide/messages.js";
import "$lib/i18n/locale.svelte";
import {
  SORT_FIELDS,
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

/** Undefined is an empty value; empty values follow all others. */
function sort_key(task: Task, field: SortField): SortKey | undefined {
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

export function compare_tasks(
  left: Task,
  right: Task,
  rules: SortRule[],
): number {
  const pinned = Number(right.pinned ?? false) - Number(left.pinned ?? false);
  if (pinned) {
    return pinned;
  }
  for (const rule of rules) {
    if (!is_sort_field(rule.field)) {
      continue;
    }
    const left_key = sort_key(left, rule.field);
    const right_key = sort_key(right, rule.field);
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

/** The column's tasks in display order. */
export function sorted_column_tasks(column: Column): Task[] {
  if (active_sort_rules(column.sort_rules).length === 0) {
    return column.tasks;
  }
  // `sort` is stable, so ties keep the manual order.
  return [...column.tasks].sort((left, right) =>
    compare_tasks(left, right, column.sort_rules),
  );
}

export const DEFAULT_SORT_DIRECTION: Record<SortField, SortDirection> = {
  due_time: "asc",
  start_time: "asc",
  created_time: "desc",
  title: "asc",
  checklist_progress: "asc",
  priority: "desc",
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
