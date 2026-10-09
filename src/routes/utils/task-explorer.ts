export interface SearchOptions {
  candidate_limit?: number;
  result_limit?: number;
  rrf_k?: number;
}

export interface TaskExplorerQuery {
  query: string;
  board_ids: number[] | null;
  column_id: number | null;
  status: "all" | "active" | "overdue" | "recurring" | "archived";
  sort: "due" | "title" | "column" | "archived";
  due: "all" | "due" | "none";
  due_start: number | null;
  due_end: number | null;
  archive_start: number | null;
  archive_end: number | null;
  now: number;
  search_options?: SearchOptions;
}

export function date_range_bounds(
  start: string,
  end: string,
): [number | null, number | null] {
  const after_end = end ? new Date(`${end}T00:00:00`) : null;
  after_end?.setDate(after_end.getDate() + 1);
  return [
    start ? new Date(`${start}T00:00:00`).getTime() : null,
    after_end?.getTime() ?? null,
  ];
}

export function is_overdue(
  task: { due_time?: Date },
  now: Date,
  archived_at?: Date,
): boolean {
  return !archived_at && Boolean(task.due_time && task.due_time < now);
}
