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
    if (!recurrence) return "Does not repeat";
    const unit = recurrence.frequency === "daily"
        ? "day"
        : recurrence.frequency === "weekly"
          ? "week"
          : "month";
    return recurrence.interval === 1
        ? `Every ${unit}`
        : `Every ${recurrence.interval} ${unit}s`;
}

export interface Task {
    id: string;
    title: string;
    description: string;
    color: string;
    start_time: Date | undefined;
    due_time: Date | undefined;
    labels: string[];
    items: TaskItem[];
    recurrence: Recurrence | undefined;
}

export interface TaskSerialized {
    id: number;
    title: string;
    description: string;
    color: string;
    start_time: number | undefined;
    due_time: number | undefined;
    labels: string[];
    items?: TaskItem[];
    recurrence?: Recurrence;
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
    }
}

export function reset_task(task: Task): void {
    task.id = "";
    task.title = "";
    task.description = "";
    task.color = "";
    task.start_time = undefined;
    task.due_time = undefined;
    task.labels = [];
    task.items = [];
    task.recurrence = undefined;
}

export function clone_task(task: Task): Task {
    return {
        ...task,
        labels: [...task.labels],
        items: task.items.map((item) => ({ ...item })),
        recurrence: task.recurrence ? { ...task.recurrence } : undefined,
    };
}

export function duplicate_task(task: Task): Task {
    return {
        ...clone_task(task),
        id: "",
        start_time: task.start_time ? new Date(task.start_time) : undefined,
        due_time: task.due_time ? new Date(task.due_time) : undefined,
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
    result.items = result.items.map((item) => ({
        ...item,
        completed: false,
    }));
    result.recurrence = undefined;
    return result;
}

export function set_task_id(task: Task, id: number): void {
    task.id = `task_${id}`;
}

export function get_task_id(task: Task | string): number {
    const value = typeof task === "string" ? task : task.id;
    const match = value.match(/^task_(\d+)$/);
    if (!match) throw new Error(`Invalid task ID: ${value}`);
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
        labels: task.labels,
        items: task.items,
        recurrence: task.recurrence,
    }
}

export function deserialize_task(data: TaskSerialized): Task {
    let t: Task = {
        id: "",
        title: data.title,
        description: data.description,
        color: data.color,
        start_time: data.start_time ? new Date(data.start_time) : undefined,
        due_time: data.due_time ? new Date(data.due_time) : undefined,
        labels: data.labels,
        items: (data.items ?? []).map((item) => ({ ...item })),
        recurrence: data.recurrence ? { ...data.recurrence } : undefined,
    };
    set_task_id(t, data.id);
    return t;
}
