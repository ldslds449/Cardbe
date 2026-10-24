import { m } from "$lib/paraglide/messages.js";

import type { AllTaskItem } from "../../board-base.svelte";
import { priority_label, TASK_PRIORITIES } from "../../type/task.svelte";
import type { FocusGroup } from "../../type/focus";
import { due_limit } from "../../utils/task-date-range";

export interface FocusGroupItems {
  key: string;
  /** Null for the single group of an ungrouped view. */
  label: string | null;
  items: AllTaskItem[];
}

// The same buckets and names as the Today view.
const DUE_BUCKETS = ["overdue", "today", "upcoming", "unscheduled"] as const;
type DueBucket = (typeof DUE_BUCKETS)[number];

function due_bucket(item: AllTaskItem, now: Date): DueBucket {
  const due = item.task.due_time;
  if (!due) {
    return "unscheduled";
  }
  // Past its due time, or past the end of the day for a date-only due.
  if (due_limit(due, item.task.due_at_midnight) < now.getTime()) {
    return "overdue";
  }
  const tomorrow = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1,
  );
  return due < tomorrow ? "today" : "upcoming";
}

function due_bucket_label(bucket: DueBucket): string {
  return {
    overdue: m.explorer_overdue,
    today: m.workspace_today,
    upcoming: m.today_upcoming,
    unscheduled: m.today_unscheduled,
  }[bucket]();
}

/**
 * Splits results into groups in a fixed order: boards by name, priorities
 * from most to least important, due dates from overdue to undated. Items keep
 * the view's sort order within each group.
 */
export function group_focus_items(
  items: AllTaskItem[],
  group: FocusGroup,
  now: Date,
): FocusGroupItems[] {
  if (group === "none") {
    return items.length > 0 ? [{ key: "all", label: null, items }] : [];
  }
  const groups = new Map<string, FocusGroupItems>();
  const add = (key: string, label: string, item: AllTaskItem) => {
    let entry = groups.get(key);
    if (!entry) {
      entry = { key, label, items: [] };
      groups.set(key, entry);
    }
    entry.items.push(item);
  };
  for (const item of items) {
    if (group === "board") {
      add(`board_${item.board_id}`, item.board_name, item);
    } else if (group === "priority") {
      const priority = item.task.priority ?? "none";
      add(priority, priority_label(item.task.priority), item);
    } else {
      const bucket = due_bucket(item, now);
      add(bucket, due_bucket_label(bucket), item);
    }
  }
  const order =
    group === "priority"
      ? [...[...TASK_PRIORITIES].reverse(), "none"]
      : group === "due"
        ? [...DUE_BUCKETS]
        : null;
  const result = [...groups.values()];
  return order
    ? result.sort((a, b) => order.indexOf(a.key) - order.indexOf(b.key))
    : result.sort((a, b) => a.label!.localeCompare(b.label!));
}
