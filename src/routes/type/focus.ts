import { m } from "$lib/paraglide/messages.js";

import type {
  CompletionFilter,
  PriorityFilter,
  TaskExplorerQuery,
  WaitingFilter,
} from "../utils/task-explorer";

export type FocusDue =
  | "all"
  | "overdue"
  | "today"
  /** Overdue or due today. */
  | "by_today"
  /** Due today or in the following six days. */
  | "next_7_days"
  | "none";

export type FocusSort = "priority" | "due" | "created" | "title";

export type FocusGroup = "none" | "board" | "priority" | "due";

/** Conditions that all have to match; a list matches any of its values. */
export interface FocusFilter {
  query: string;
  /** Null covers every board, including boards created later. */
  board_ids: number[] | null;
  /** Empty keeps all. */
  completion: CompletionFilter[];
  /** Empty keeps all. */
  priorities: PriorityFilter[];
  due: FocusDue;
  /** Any of these labels; empty keeps all. */
  labels: string[];
  waiting: WaitingFilter | null;
  recurring: boolean | null;
}

/** Saved on this device only, outside synchronization and Undo. */
export interface Focus {
  id: string;
  name: string;
  filter: FocusFilter;
  sort: FocusSort;
  group: FocusGroup;
}

export const MAX_FOCUSES = 100;
export const MAX_FOCUS_NAME_LENGTH = 100;

/**
 * The built-in Today Focus. It is never saved, cannot be edited or deleted,
 * and the backend refuses saved views with this ID.
 */
export const TODAY_FOCUS_ID = "builtin.today";

/** Open tasks of every board, grouped from overdue to undated. */
export function today_focus(): Focus {
  return {
    id: TODAY_FOCUS_ID,
    name: m.workspace_today(),
    filter: create_focus_filter(),
    sort: "due",
    group: "due",
  };
}

export function is_builtin_focus(id: string): boolean {
  return id === TODAY_FOCUS_ID;
}

export function create_focus_filter(): FocusFilter {
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

export function create_focus(name: string): Focus {
  return {
    id: globalThis.crypto.randomUUID(),
    name,
    filter: create_focus_filter(),
    sort: "priority",
    group: "none",
  };
}

export function clone_focus(view: Focus): Focus {
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

function day_start(date: Date, offset_days = 0): number {
  const start = new Date(date);
  start.setHours(0, 0, 0, 0);
  start.setDate(start.getDate() + offset_days);
  return start.getTime();
}

/** The task explorer query for a view, with dates relative to `now`. */
export function focus_query(view: Focus, now = new Date()): TaskExplorerQuery {
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

/** The task explorer conditions that a Focus can keep. */
export interface ExplorerConditions {
  query: string;
  board_ids: number[] | null;
  status: TaskExplorerQuery["status"];
  due: "all" | "due" | "none" | "today" | "custom";
  priorities: PriorityFilter[];
  sort: TaskExplorerQuery["sort"];
  /** A column filter or archive date range, which a Focus cannot keep. */
  other_conditions: boolean;
}

/**
 * A new unnamed Focus with the explorer's conditions. `partial` is set when
 * some conditions have no Focus equivalent and were left out.
 */
export function focus_from_explorer(conditions: ExplorerConditions): {
  view: Focus;
  partial: boolean;
} {
  const { status, due, sort } = conditions;
  const filter: FocusFilter = {
    ...create_focus_filter(),
    query: conditions.query.trim(),
    board_ids: conditions.board_ids ? [...conditions.board_ids] : null,
    // Every status but archived and all leaves archived tasks out.
    completion:
      status === "archived"
        ? ["archived"]
        : status === "active" || status === "recurring"
          ? ["open", "done"]
          : [],
    priorities: [...conditions.priorities],
    due:
      status === "overdue"
        ? "overdue"
        : due === "none" || due === "today"
          ? due
          : "all",
    recurring: status === "recurring" ? true : null,
  };
  const focus_sort: FocusSort =
    sort === "due" ||
    sort === "title" ||
    sort === "priority" ||
    sort === "created"
      ? sort
      : "priority";
  return {
    view: { ...create_focus(""), filter, sort: focus_sort },
    partial:
      conditions.other_conditions ||
      due === "due" ||
      due === "custom" ||
      // Overdue already restricts due dates.
      (status === "overdue" && due !== "all") ||
      focus_sort !== sort,
  };
}
