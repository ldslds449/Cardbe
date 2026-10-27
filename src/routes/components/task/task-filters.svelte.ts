import { m } from "#lib/paraglide/messages.js";

import { get_column_id } from "../../type/column.svelte";
import type { ExplorerConditions, FocusGroup } from "../../type/focus";
import {
  date_range_bounds,
  type PriorityFilter,
  type TaskExplorerQuery,
} from "../../utils/task-explorer";

export type TaskStatus = TaskExplorerQuery["status"];
export type DueFilter = "all" | "due" | "none" | "today" | "custom";
export type SortMode =
  | "due"
  | "title"
  | "column"
  | "archived"
  | "priority"
  | "relevance";
export type ArchiveTimeFilter =
  | "all"
  | "today"
  | "7d"
  | "30d"
  | "year"
  | "custom";

/** Plain conditions, kept apart from the reactive class so they can be copied. */
export interface TaskFilterValues {
  status: TaskStatus;
  board_ids: number[] | null;
  search_text: string;
  /** A column ID of the single selected board, or "all". */
  column: string;
  due: DueFilter;
  due_start: string;
  due_end: string;
  priorities: PriorityFilter[];
  sort: SortMode;
  /** Searches sort by relevance until another order is picked for them. */
  prefer_relevance: boolean;
  archive_time: ArchiveTimeFilter;
  archive_start: string;
  archive_end: string;
  group: FocusGroup;
}

export function default_task_filter_values(): TaskFilterValues {
  return {
    status: "active",
    board_ids: null,
    search_text: "",
    column: "all",
    due: "all",
    due_start: "",
    due_end: "",
    priorities: [],
    sort: "due",
    prefer_relevance: true,
    archive_time: "all",
    archive_start: "",
    archive_end: "",
    group: "none",
  };
}

export function effective_sort(values: TaskFilterValues): SortMode {
  return values.search_text.trim() && values.prefer_relevance
    ? "relevance"
    : values.sort;
}

export function board_scope_name(
  board_ids: number[] | null,
  boards: { id: number; name: string }[],
): string {
  return board_ids === null
    ? m.explorer_all_boards()
    : board_ids.length === 0
      ? m.explorer_no_boards()
      : board_ids.length === 1
        ? (boards.find((item) => item.id === board_ids[0])?.name ??
          m.explorer_board_fallback())
        : m.explorer_selected({ count: board_ids.length });
}

/** Sort orders offered for a status; relevance only while searching. */
export function sort_options(
  status: TaskStatus,
  searching: boolean,
): { id: SortMode; label: string }[] {
  const relevance = searching
    ? [{ id: "relevance" as const, label: m.explorer_relevance() }]
    : [];
  return [
    ...relevance,
    ...(status === "archived"
      ? [
          { id: "archived" as const, label: m.explorer_archived_at() },
          { id: "title" as const, label: m.task_title() },
        ]
      : status === "recurring"
        ? [
            { id: "title" as const, label: m.task_title() },
            { id: "column" as const, label: m.explorer_column() },
            { id: "priority" as const, label: m.task_priority() },
          ]
        : [
            { id: "due" as const, label: m.explorer_due_label() },
            { id: "priority" as const, label: m.task_priority() },
            { id: "title" as const, label: m.task_title() },
            { id: "column" as const, label: m.explorer_column() },
          ]),
  ];
}

/** The column filter applies only while a single board is selected. */
function column_id(values: TaskFilterValues): number | null {
  return values.board_ids?.length === 1 &&
    values.status !== "archived" &&
    values.column !== "all"
    ? get_column_id(values.column)
    : null;
}

/** The `list_all_tasks` filter, with relative dates taken at `now`. */
export function task_filter_query(
  values: TaskFilterValues,
  now: Date,
): TaskExplorerQuery {
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const tomorrow = new Date(today);
  tomorrow.setDate(tomorrow.getDate() + 1);
  const [due_from, due_until] = date_range_bounds(
    values.due_start,
    values.due_end,
  );
  let [archive_from, archive_until] = date_range_bounds(
    values.archive_start,
    values.archive_end,
  );
  if (values.archive_time !== "custom") {
    const start = new Date(today);
    if (values.archive_time === "7d") {
      start.setDate(start.getDate() - 6);
    }
    if (values.archive_time === "30d") {
      start.setDate(start.getDate() - 29);
    }
    if (values.archive_time === "year") {
      start.setMonth(0, 1);
    }
    archive_from = values.archive_time === "all" ? null : start.getTime();
    archive_until = values.archive_time === "all" ? null : tomorrow.getTime();
  }
  const due = values.due;
  return {
    query: values.search_text.trim(),
    board_ids: values.board_ids,
    column_id: column_id(values),
    status: values.status,
    sort: effective_sort(values),
    due: due === "today" || due === "custom" ? "due" : due,
    due_start:
      due === "today" ? today.getTime() : due === "custom" ? due_from : null,
    due_end:
      due === "today"
        ? tomorrow.getTime()
        : due === "custom"
          ? due_until
          : null,
    archive_start: archive_from,
    archive_end: archive_until,
    now: now.getTime(),
    priorities: values.priorities.length > 0 ? values.priorities : null,
  };
}

/** Explorer conditions with a reactive copy of each value. */
export class TaskFilters {
  status = $state<TaskStatus>("active");
  board_ids = $state<number[] | null>(null);
  #search_text = $state("");
  column = $state("all");
  due = $state<DueFilter>("all");
  due_start = $state("");
  due_end = $state("");
  priorities = $state<PriorityFilter[]>([]);
  sort = $state<SortMode>("due");
  prefer_relevance = $state(true);
  archive_time = $state<ArchiveTimeFilter>("all");
  archive_start = $state("");
  archive_end = $state("");
  group = $state<FocusGroup>("none");

  get search_text() {
    return this.#search_text;
  }

  set search_text(value: string) {
    this.#search_text = value;
    // A new search starts sorted by relevance again.
    if (!value.trim()) {
      this.prefer_relevance = true;
    }
  }

  readonly searching = $derived(Boolean(this.#search_text.trim()));
  readonly values = $derived<TaskFilterValues>({
    status: this.status,
    board_ids: this.board_ids,
    search_text: this.#search_text,
    column: this.column,
    due: this.due,
    due_start: this.due_start,
    due_end: this.due_end,
    priorities: this.priorities,
    sort: this.sort,
    prefer_relevance: this.prefer_relevance,
    archive_time: this.archive_time,
    archive_start: this.archive_start,
    archive_end: this.archive_end,
    group: this.group,
  });
  readonly effective_sort = $derived(effective_sort(this.values));
  readonly sort_options = $derived(sort_options(this.status, this.searching));
  /** Filters beyond search, scope, and status. */
  readonly active_filter_count = $derived(
    Number(column_id(this.values) !== null) +
      Number(this.due !== "all") +
      Number(this.priorities.length > 0) +
      Number(this.archive_time !== "all"),
  );

  set_status(status: TaskStatus) {
    this.status = status;
    if (status === "archived") {
      this.column = "all";
    }
    if (status !== "archived" && status !== "all") {
      this.clear_archive_time();
    }
    const options = sort_options(status, false);
    if (!options.some((option) => option.id === this.sort)) {
      this.sort = options[0].id;
    }
  }

  set_sort(sort: SortMode) {
    this.prefer_relevance = sort === "relevance";
    if (sort !== "relevance") {
      this.sort = sort;
    }
  }

  set_board_ids(board_ids: number[] | null) {
    this.board_ids = board_ids;
    this.column = "all";
  }

  clear_due() {
    this.due = "all";
    this.due_start = "";
    this.due_end = "";
  }

  clear_archive_time() {
    this.archive_time = "all";
    this.archive_start = "";
    this.archive_end = "";
  }

  reset_filters() {
    this.column = "all";
    this.priorities = [];
    this.clear_due();
    this.clear_archive_time();
  }

  clear() {
    this.search_text = "";
    this.reset_filters();
  }

  /** Replaces every condition with `values`, such as another view's. */
  assign(values: TaskFilterValues) {
    this.status = values.status;
    this.board_ids = values.board_ids ? [...values.board_ids] : null;
    this.#search_text = values.search_text;
    this.column = values.column;
    this.due = values.due;
    this.due_start = values.due_start;
    this.due_end = values.due_end;
    this.priorities = [...values.priorities];
    this.sort = values.sort;
    this.prefer_relevance = values.prefer_relevance;
    this.archive_time = values.archive_time;
    this.archive_start = values.archive_start;
    this.archive_end = values.archive_end;
    this.group = values.group;
  }

  query(now: Date): TaskExplorerQuery {
    return task_filter_query(this.values, now);
  }

  focus_conditions(): ExplorerConditions {
    return {
      query: this.#search_text,
      board_ids: this.board_ids,
      status: this.status,
      due: this.due,
      priorities: this.priorities,
      sort: this.effective_sort,
      other_conditions:
        column_id(this.values) !== null || this.archive_time !== "all",
    };
  }
}
