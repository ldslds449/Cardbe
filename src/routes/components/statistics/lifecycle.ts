import type { Archive } from "../../type/archive.svelte";
import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import { due_limit } from "../../utils/task-date-range";

const DAY = 24 * 60 * 60 * 1000;

export type LifecycleRange = 7 | 30 | 90 | 365 | "all";
export const lifecycle_ranges: LifecycleRange[] = [7, 30, 90, 365, "all"];

// Upper bounds of the lead time buckets; the last bucket is open-ended.
export const lead_time_bucket_limits = [1, 3, 7, 14, 30].map((d) => d * DAY);

export interface AgingTask {
  task: Task;
  column_name: string;
  age: number;
  /** Milliseconds past the due time, or `null` when not overdue. */
  overdue: number | null;
}

export interface LabelLifecycle {
  /** `""` groups tasks without labels. */
  label: string;
  completed: number;
  median: number | null;
  due_completed: number;
  on_time: number;
  /** Active tasks past their due time, regardless of the range. */
  overdue: number;
}

export function percentile(sorted: number[], p: number): number | null {
  if (sorted.length === 0) {
    return null;
  }
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
}

export function median(sorted: number[]): number | null {
  if (sorted.length === 0) {
    return null;
  }
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2
    ? sorted[middle]
    : (sorted[middle - 1] + sorted[middle]) / 2;
}

function lead_time(archive: Archive): number | null {
  const created = archive.task.created_time?.getTime();
  if (!created || created > archive.time.getTime()) {
    return null;
  }
  return archive.time.getTime() - created;
}

function on_time(archive: Archive): boolean {
  const due = archive.task.due_time;
  return (
    !!due &&
    archive.time.getTime() <= due_limit(due, archive.task.due_at_midnight)
  );
}

function task_labels(task: Task): string[] {
  return task.labels.length ? task.labels : [""];
}

function week_start(time: Date): Date {
  const start = new Date(time.getFullYear(), time.getMonth(), time.getDate());
  start.setDate(start.getDate() - start.getDay());
  return start;
}

/** Archived tasks count as completed; restored or deleted tasks are absent. */
export function task_lifecycle(
  archives: Archive[],
  columns: Column[],
  range: LifecycleRange,
  now = new Date(),
) {
  const end = now.getTime();
  const start = range === "all" ? -Infinity : end - range * DAY;
  const previous_start = range === "all" ? null : start - range * DAY;

  const archived = archives.filter((archive) => archive.time.getTime() <= end);
  const completed = archived.filter(
    (archive) => archive.time.getTime() > start,
  );
  const previous_completed =
    previous_start === null
      ? null
      : archived.filter((archive) => {
          const time = archive.time.getTime();
          return time > previous_start && time <= start;
        }).length;

  const lead_times = completed
    .map(lead_time)
    .filter((value) => value !== null)
    .sort((a, b) => a - b);
  const distribution = lead_time_bucket_limits.map(() => 0).concat(0);
  for (const value of lead_times) {
    const index = lead_time_bucket_limits.findIndex((limit) => value < limit);
    distribution[index < 0 ? lead_time_bucket_limits.length : index]++;
  }

  const with_due = completed.filter((archive) => archive.task.due_time);

  const active = columns.flatMap((column) =>
    column.tasks.map((task) => ({ task, column_name: column.name })),
  );
  const overdue_time = (task: Task) => {
    if (!task.due_time) {
      return null;
    }
    const limit = due_limit(task.due_time, task.due_at_midnight);
    return limit < end ? end - limit : null;
  };
  const aging: AgingTask[] = active
    .filter(
      ({ task }) => task.created_time && task.created_time.getTime() <= end,
    )
    .map(({ task, column_name }) => ({
      task,
      column_name,
      age: end - task.created_time!.getTime(),
      overdue: overdue_time(task),
    }))
    .sort((a, b) => b.age - a.age || Number(a.task.id) - Number(b.task.id));

  // A task with several labels counts once under each of them.
  const labels = new Map<string, LabelLifecycle & { times: number[] }>();
  const row = (label: string) => {
    let entry = labels.get(label);
    if (!entry) {
      entry = {
        label,
        completed: 0,
        median: null,
        due_completed: 0,
        on_time: 0,
        overdue: 0,
        times: [],
      };
      labels.set(label, entry);
    }
    return entry;
  };
  for (const archive of completed) {
    const time = lead_time(archive);
    for (const label of task_labels(archive.task)) {
      const entry = row(label);
      entry.completed++;
      entry.due_completed += archive.task.due_time ? 1 : 0;
      entry.on_time += on_time(archive) ? 1 : 0;
      if (time !== null) {
        entry.times.push(time);
      }
    }
  }
  for (const { task } of active) {
    if (overdue_time(task) !== null) {
      for (const label of task_labels(task)) {
        row(label).overdue++;
      }
    }
  }
  const by_label: LabelLifecycle[] = [...labels.values()]
    .map(({ times, ...entry }) => ({
      ...entry,
      median: median(times.sort((a, b) => a - b)),
    }))
    .sort(
      (a, b) =>
        b.completed - a.completed ||
        b.overdue - a.overdue ||
        a.label.localeCompare(b.label),
    );

  return {
    completed: completed.length,
    previous_completed,
    lead_time_median: median(lead_times),
    lead_time_p85: percentile(lead_times, 0.85),
    lead_time_count: lead_times.length,
    unknown_created: completed.length - lead_times.length,
    distribution,
    due_completed: with_due.length,
    on_time: with_due.filter(on_time).length,
    overdue: active.filter(({ task }) => overdue_time(task) !== null).length,
    aging,
    by_label,
    weekly: weekly_flow(
      archived,
      active.map(({ task }) => task),
      range,
      now,
    ),
  };
}

/** Tasks created and completed per Sunday-based week, oldest first. */
function weekly_flow(
  archived: Archive[],
  active: Task[],
  range: LifecycleRange,
  now: Date,
) {
  const count =
    range === "all" ? 52 : Math.min(52, Math.max(12, Math.ceil(range / 7)));
  const first = week_start(now);
  first.setDate(first.getDate() - (count - 1) * 7);
  const weeks = Array.from({ length: count }, (_, index) => {
    const start = new Date(first);
    start.setDate(start.getDate() + index * 7);
    return { start, created: 0, completed: 0 };
  });
  const index_of = (time: Date) => {
    if (time > now || time < first) {
      return -1;
    }
    // Calendar weeks avoid DST drift from dividing by a fixed week length.
    const start = week_start(time).getTime();
    return weeks.findIndex((week) => week.start.getTime() === start);
  };
  for (const task of [...active, ...archived.map((archive) => archive.task)]) {
    const index = task.created_time ? index_of(task.created_time) : -1;
    if (index >= 0) {
      weeks[index].created++;
    }
  }
  for (const archive of archived) {
    const index = index_of(archive.time);
    if (index >= 0) {
      weeks[index].completed++;
    }
  }
  return weeks;
}
