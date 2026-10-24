import { invoke } from "@tauri-apps/api/core";

import type { AllTaskItem } from "../../board-base.svelte";
import { focus_query, type Focus } from "../../type/focus";
import {
  deserialize_task_summary,
  type TaskSummarySerialized,
} from "../../type/task.svelte";

interface PageSerialized {
  items: Array<{
    board_id: number;
    board_name: string;
    column_id: number | null;
    column_name: string | null;
    archived_at: number | null;
    done: boolean;
    waiting_count: number;
    task: TaskSummarySerialized;
  }>;
  next_cursor: string | null;
}

export interface FocusResults {
  items: AllTaskItem[];
  next_cursor: string | null;
}

/** Reads one page of a view's tasks, with relative dates taken at `now`. */
export async function load_focus_results(
  view: Focus,
  now: Date,
  cursor: string | null,
  limit: number,
): Promise<FocusResults> {
  const page = await invoke<PageSerialized>("list_all_tasks", {
    cursor,
    filter: focus_query(view, now),
    limit,
  });
  return {
    items: page.items.map((item) => ({
      board_id: item.board_id,
      board_name: item.board_name,
      column_id:
        item.column_id === null ? undefined : `column_${item.column_id}`,
      column_name: item.column_name ?? undefined,
      archived_at:
        item.archived_at === null ? undefined : new Date(item.archived_at),
      done: item.done,
      waiting_count: item.waiting_count,
      task: deserialize_task_summary(item.task),
    })),
    next_cursor: page.next_cursor,
  };
}
