import type { Archive } from "../../type/archive.svelte";
import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import { due_limit } from "../../utils/task-date-range";

const DAY = 24 * 60 * 60 * 1000;

export type LifecycleRange = 7 | 30 | 90 | 365 | "all";
export const lifecycle_ranges: LifecycleRange[] = [7, 30, 90, 365, "all"];

// Upper bounds of the lead time buckets; the last bucket is open-ended.
export const lead_time_bucket_limits = [1, 3, 7, 14, 30].map((d) => d * DAY);

export interface LifecycleFilter {
  range: LifecycleRange;
  /** `null` keeps all tasks; `""` keeps tasks without labels. */
  label: string | null;
}

export interface AgingTask {
  task: Task;
  column_name: string;
  age: number;
  /** Milliseconds past the due time, or `null` when not overdue. */
  overdue: number | null;
}

export interface LabelLeadTime {
  label: string;
  completed: number;
  median: number | null;
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

function matches_label(task: Task, label: string | null): boolean {
  return (
    label === null ||
    (label === "" ? task.labels.length === 0 : task.labels.includes(label))
  );
}

function lead_time(archive: Archive): number | null {
  const created = archive.task.created_time?.getTime();
  if (!created || created > archive.time.getTime()) {
    return null;
  }
  return archive.time.getTime() - created;
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
  filter: LifecycleFilter,
  now = new Date(),
) {
  const end = now.getTime();
  const start = filter.range === "all" ? -Infinity : end - filter.range * DAY;
  const previous_start =
    filter.range === "all" ? null : start - filter.range * DAY;

  const archived = archives.filter(
    (archive) =>
      archive.time.getTime() <= end &&
      matches_label(archive.task, filter.label),
  );
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
  const on_time = with_due.filter(
    (archive) =>
      archive.time.getTime() <=
      due_limit(archive.task.due_time!, archive.task.due_at_midnight),
  ).length;

  const active = columns.flatMap((column) =>
    column.tasks
      .filter((task) => matches_label(task, filter.label))
      .map((task) => ({ task, column_name: column.name })),
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

  const label_times = new Map<string, number[]>();
  const label_counts = new Map<string, number>();
  for (const archive of completed) {
    const time = lead_time(archive);
    for (const label of archive.task.labels.length
      ? archive.task.labels
      : [""]) {
      label_counts.set(label, (label_counts.get(label) ?? 0) + 1);
      if (time !== null) {
        label_times.set(label, [...(label_times.get(label) ?? []), time]);
      }
    }
  }
  const by_label: LabelLeadTime[] = [...label_counts]
    .map(([label, count]) => ({
      label,
      completed: count,
      median: median((label_times.get(label) ?? []).sort((a, b) => a - b)),
    }))
    .sort(
      (a, b) => b.completed - a.completed || a.label.localeCompare(b.label),
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
    on_time,
    overdue: active.filter(({ task }) => overdue_time(task) !== null).length,
    aging,
    by_label,
    weekly: weekly_flow(
      archived,
      active.map(({ task }) => task),
      filter,
      now,
    ),
  };
}

/** Tasks created and completed per Sunday-based week, oldest first. */
function weekly_flow(
  archived: Archive[],
  active: Task[],
  filter: LifecycleFilter,
  now: Date,
) {
  const count =
    filter.range === "all"
      ? 52
      : Math.min(52, Math.max(12, Math.ceil(filter.range / 7)));
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

export function lifecycle_labels(
  archives: Archive[],
  columns: Column[],
): string[] {
  return [
    ...new Set([
      ...archives.flatMap((archive) => archive.task.labels),
      ...columns.flatMap((column) =>
        column.tasks.flatMap((task) => task.labels),
      ),
    ]),
  ].sort((a, b) => a.localeCompare(b));
}
