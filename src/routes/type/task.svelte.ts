import { m } from "#lib/paraglide/messages.js";
import "#lib/i18n/locale.svelte.js";

export interface TaskItem {
  id: string;
  text: string;
  completed: boolean;
}

export type RecurrenceFrequency = "daily" | "weekly" | "monthly";

export interface Recurrence {
  frequency: RecurrenceFrequency;
  interval: number;
}

export function recurrence_label(recurrence: Recurrence | undefined): string {
  if (!recurrence) {
    return m.task_repeat_none();
  }
  const inputs = { count: recurrence.interval };
  return recurrence.frequency === "daily"
    ? m.task_repeat_day_count(inputs)
    : recurrence.frequency === "weekly"
      ? m.task_repeat_week_count(inputs)
      : m.task_repeat_month_count(inputs);
}

/** Ordered from least to most important; unset means no priority. */
export const TASK_PRIORITIES = ["low", "medium", "high", "urgent"] as const;

export type TaskPriority = (typeof TASK_PRIORITIES)[number];

export function is_task_priority(value: unknown): value is TaskPriority {
  return TASK_PRIORITIES.includes(value as TaskPriority);
}

export function priority_label(priority: TaskPriority | undefined): string {
  return priority === "low"
    ? m.task_priority_low()
    : priority === "medium"
      ? m.task_priority_medium()
      : priority === "high"
        ? m.task_priority_high()
        : priority === "urgent"
          ? m.task_priority_urgent()
          : m.task_priority_none();
}

export interface Task {
  pinned?: boolean;
  id: string;
  title: string;
  description: string;
  color: string;
  start_time: Date | undefined;
  due_time: Date | undefined;
  /**
   * A time at local midnight is a date without a time of day unless these
   * mark it as a chosen 00:00; see `utils/task-date-range`.
   */
  start_at_midnight?: boolean;
  due_at_midnight?: boolean;
  created_time?: Date;
  labels: string[];
  items: TaskItem[];
  recurrence: Recurrence | undefined;
  priority?: TaskPriority;
  /**
   * When the task entered the board's done column. The backend derives it
   * from the task's position; archived tasks completed when archived.
   */
  completed_at?: Date;
  /** Tasks of the same board this one waits on; see `utils/blocking`. */
  blocked_by?: string[];
}

export interface TaskSerialized {
  pinned?: boolean;
  id: number;
  title: string;
  description: string;
  color: string;
  start_time: number | undefined;
  due_time: number | undefined;
  start_at_midnight?: boolean;
  due_at_midnight?: boolean;
  created_time?: number;
  labels: string[];
  items?: TaskItem[];
  recurrence?: Recurrence;
  priority?: TaskPriority;
  completed_at?: number;
  blocked_by?: number[];
}

export interface TaskSummary {
  id: string;
  title: string;
  color: string;
  start_time: Date | undefined;
  due_time: Date | undefined;
  start_at_midnight?: boolean;
  due_at_midnight?: boolean;
  created_time?: Date;
  labels: string[];
  recurrence: Recurrence | undefined;
  priority?: TaskPriority;
}

export interface TaskSummarySerialized {
  id: number;
  title: string;
  color: string;
  start_time: number;
  due_time: number | undefined;
  start_at_midnight?: boolean;
  due_at_midnight?: boolean;
  created_time?: number;
  labels: string[];
  recurrence?: Recurrence;
  priority?: TaskPriority;
}

export interface TaskTemplate {
  id: number;
  name: string;
  task: Task;
}

export interface TaskTemplateSerialized {
  id: number;
  name: string;
  task: TaskSerialized;
}

export function create_task_item(text: string = ""): TaskItem {
  return {
    id: globalThis.crypto.randomUUID(),
    text,
    completed: false,
  };
}

export function create_task(
  id: string = "",
  title: string = "",
  description: string = "",
  color: string = "",
  start_time: Date | undefined = undefined,
  due_time: Date | undefined = undefined,
  labels: string[] = [],
  items: TaskItem[] = [],
  recurrence: Recurrence | undefined = undefined,
): Task {
  return {
    id: id,
    title: title,
    description: description,
    color: color,
    start_time: start_time,
    due_time: due_time,
    labels: labels,
    items: items,
    recurrence: recurrence,
    pinned: false,
  };
}

export function reset_task(task: Task): void {
  task.id = "";
  task.title = "";
  task.description = "";
  task.color = "";
  task.start_time = undefined;
  task.due_time = undefined;
  task.start_at_midnight = undefined;
  task.due_at_midnight = undefined;
  task.created_time = undefined;
  task.labels = [];
  task.items = [];
  task.recurrence = undefined;
  task.priority = undefined;
  task.completed_at = undefined;
  task.blocked_by = undefined;
  task.pinned = false;
}

export function clone_task(task: Task): Task {
  return {
    ...task,
    labels: [...task.labels],
    items: task.items.map((item) => ({ ...item })),
    recurrence: task.recurrence ? { ...task.recurrence } : undefined,
    blocked_by: task.blocked_by ? [...task.blocked_by] : undefined,
  };
}

export function duplicate_task(task: Task): Task {
  return {
    ...clone_task(task),
    id: "",
    start_time: task.start_time ? new Date(task.start_time) : undefined,
    due_time: task.due_time ? new Date(task.due_time) : undefined,
    created_time: undefined,
    items: task.items.map((item) => ({
      ...item,
      id: globalThis.crypto.randomUUID(),
    })),
  };
}

export function task_from_template(task: Task): Task {
  const result = duplicate_task(task);
  result.start_time = undefined;
  result.due_time = undefined;
  result.start_at_midnight = undefined;
  result.due_at_midnight = undefined;
  result.items = result.items.map((item) => ({
    ...item,
    completed: false,
  }));
  result.recurrence = undefined;
  result.blocked_by = undefined;
  result.pinned = false;
  return result;
}

export function set_task_id(task: Task, id: number): void {
  task.id = `task_${id}`;
}

export function get_task_id(task: Task | string): number {
  const value = typeof task === "string" ? task : task.id;
  const match = value.match(/^task_(\d+)$/);
  if (!match) {
    throw new Error(m.task_id_invalid({ value }));
  }
  return Number.parseInt(match[1], 10);
}

export function serialize_task(task: Task): TaskSerialized {
  const idMatch = task.id.match(/^task_(\d+)$/);
  return {
    id: idMatch ? parseInt(idMatch[1]) : -1,
    title: task.title,
    description: task.description,
    color: task.color,
    start_time: task.start_time?.getTime(),
    due_time: task.due_time?.getTime(),
    ...midnight_flags(task),
    created_time: task.created_time?.getTime(),
    labels: task.labels,
    items: task.items,
    recurrence: task.recurrence,
    ...priority_field(task),
    // Sent back unchanged so edits keep the original completion time.
    ...(task.completed_at ? { completed_at: task.completed_at.getTime() } : {}),
    ...(task.blocked_by?.length
      ? { blocked_by: task.blocked_by.map(get_task_id) }
      : {}),
    pinned: task.pinned ?? false,
  };
}

/** The flags only when set, so other tasks keep their stored shape. */
function midnight_flags(
  task: Pick<Task, "start_at_midnight" | "due_at_midnight">,
): Pick<Task, "start_at_midnight" | "due_at_midnight"> {
  return {
    ...(task.start_at_midnight ? { start_at_midnight: true } : {}),
    ...(task.due_at_midnight ? { due_at_midnight: true } : {}),
  };
}

/** Only a known priority, so unset and unknown values are omitted. */
function priority_field(task: Pick<Task, "priority">): Pick<Task, "priority"> {
  return is_task_priority(task.priority) ? { priority: task.priority } : {};
}

function deserialize_task_times(
  start_time: number | undefined,
  created_time: number | undefined,
): Pick<Task, "start_time" | "created_time"> {
  // Tasks saved before start times became editable stored their creation
  // time in start_time; they have no user-chosen start.
  if (!created_time) {
    return {
      start_time: undefined,
      created_time: start_time ? new Date(start_time) : undefined,
    };
  }
  return {
    start_time: start_time ? new Date(start_time) : undefined,
    created_time: new Date(created_time),
  };
}

export function deserialize_task(data: TaskSerialized): Task {
  let t: Task = {
    id: "",
    title: data.title,
    description: data.description,
    color: data.color,
    ...deserialize_task_times(data.start_time, data.created_time),
    due_time: data.due_time ? new Date(data.due_time) : undefined,
    ...midnight_flags(data),
    labels: data.labels,
    items: (data.items ?? []).map((item) => ({ ...item })),
    recurrence: data.recurrence ? { ...data.recurrence } : undefined,
    ...priority_field(data),
    ...(data.completed_at ? { completed_at: new Date(data.completed_at) } : {}),
    ...(data.blocked_by?.length
      ? { blocked_by: data.blocked_by.map((id) => `task_${id}`) }
      : {}),
    pinned: data.pinned ?? false,
  };
  set_task_id(t, data.id);
  return t;
}

export function deserialize_task_summary(
  data: TaskSummarySerialized,
): TaskSummary {
  return {
    id: `task_${data.id}`,
    title: data.title,
    color: data.color,
    ...deserialize_task_times(data.start_time, data.created_time),
    due_time: data.due_time ? new Date(data.due_time) : undefined,
    ...midnight_flags(data),
    labels: data.labels,
    recurrence: data.recurrence ? { ...data.recurrence } : undefined,
    ...priority_field(data),
  };
}
