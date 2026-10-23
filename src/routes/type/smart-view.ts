import { m } from "$lib/paraglide/messages.js";

import type {
  CompletionFilter,
  PriorityFilter,
  TaskExplorerQuery,
  WaitingFilter,
} from "../utils/task-explorer";

export type SmartViewDue =
  | "all"
  | "overdue"
  | "today"
  /** Overdue or due today. */
  | "by_today"
  /** Due today or in the following six days. */
  | "next_7_days"
  | "none";

export type SmartViewSort = "priority" | "due" | "created" | "title";

export type SmartViewGroup = "none" | "board" | "priority" | "due";

/** Conditions that all have to match; a list matches any of its values. */
export interface SmartViewFilter {
  query: string;
  /** Null covers every board, including boards created later. */
  board_ids: number[] | null;
  /** Empty keeps all. */
  completion: CompletionFilter[];
  /** Empty keeps all. */
  priorities: PriorityFilter[];
  due: SmartViewDue;
  /** Any of these labels; empty keeps all. */
  labels: string[];
  waiting: WaitingFilter | null;
  recurring: boolean | null;
}

/** Saved on this device only, outside synchronization and Undo. */
export interface SmartView {
  id: string;
  name: string;
  filter: SmartViewFilter;
  sort: SmartViewSort;
  group: SmartViewGroup;
}

export const MAX_SMART_VIEWS = 100;
export const MAX_SMART_VIEW_NAME_LENGTH = 100;

export function create_smart_view_filter(): SmartViewFilter {
  return {
    query: "",
    board_ids: null,
    completion: ["open"],
    priorities: [],
    due: "all",
    labels: [],
    waiting: null,
    recurring: null,
  };
}

export function create_smart_view(name: string): SmartView {
  return {
    id: globalThis.crypto.randomUUID(),
    name,
    filter: create_smart_view_filter(),
    sort: "priority",
    group: "none",
  };
}

export function clone_smart_view(view: SmartView): SmartView {
  return {
    ...view,
    filter: {
      ...view.filter,
      board_ids: view.filter.board_ids ? [...view.filter.board_ids] : null,
      completion: [...view.filter.completion],
      priorities: [...view.filter.priorities],
      labels: [...view.filter.labels],
    },
  };
}

/** Offered the first time, before any view was saved. */
export function starter_smart_views(): SmartView[] {
  const view = (
    name: string,
    filter: Partial<SmartViewFilter>,
    sort: SmartViewSort,
    group: SmartViewGroup,
  ): SmartView => ({
    ...create_smart_view(name),
    filter: { ...create_smart_view_filter(), ...filter },
    sort,
    group,
  });
  return [
    view(m.smart_view_starter_due(), { due: "by_today" }, "due", "none"),
    view(
      m.smart_view_starter_important(),
      { priorities: ["urgent", "high"] },
      "priority",
      "board",
    ),
    view(
      m.smart_view_starter_waiting(),
      { waiting: "waiting" },
      "priority",
      "board",
    ),
  ];
}

function day_start(date: Date, offset_days = 0): number {
  const start = new Date(date);
  start.setHours(0, 0, 0, 0);
  start.setDate(start.getDate() + offset_days);
  return start.getTime();
}

/** The task explorer query for a view, with dates relative to `now`. */
export function smart_view_query(
  view: SmartView,
  now = new Date(),
): TaskExplorerQuery {
  const { filter } = view;
  const due = filter.due;
  const tomorrow = day_start(now, 1);
  return {
    query: filter.query,
    board_ids: filter.board_ids,
    column_id: null,
    status: due === "overdue" ? "overdue" : "all",
    sort: view.sort,
    due: due === "none" ? "none" : due === "all" ? "all" : "due",
    due_start: due === "today" || due === "next_7_days" ? day_start(now) : null,
    due_end:
      due === "today" || due === "by_today"
        ? tomorrow
        : due === "next_7_days"
          ? day_start(now, 7)
          : null,
    archive_start: null,
    archive_end: null,
    now: now.getTime(),
    priorities: filter.priorities.length > 0 ? filter.priorities : null,
    completion: filter.completion.length > 0 ? filter.completion : null,
    labels: filter.labels.length > 0 ? filter.labels : null,
    waiting: filter.waiting,
    recurring: filter.recurring,
  };
}
