import { logger } from "#lib/logger.js";

import {
  load_task_counts,
  type TaskStatusCounts,
} from "../focus/focus-results";
import {
  task_filter_query,
  type TaskFilterValues,
} from "./task-filters.svelte";

/** Conditions that change the counts; status and order do not. */
export function task_counts_key(values: TaskFilterValues): string {
  return JSON.stringify({
    ...values,
    status: null,
    sort: null,
    prefer_relevance: null,
    group: null,
  });
}

/** Task counts per status for the conditions last loaded. */
export class TaskCounts {
  value = $state<TaskStatusCounts | null>(null);
  #request = 0;

  /** Reads the counts for `values`, dropping answers to older requests. */
  async load(values: TaskFilterValues) {
    const request = ++this.#request;
    // Every status keeps the column filter, which archived drops itself.
    const filter = task_filter_query({ ...values, status: "all" }, new Date());
    try {
      const counts = await load_task_counts(filter);
      if (request === this.#request) {
        this.value = counts;
      }
    } catch (error) {
      if (request === this.#request) {
        // The list still works; its status choices only lose their counts.
        logger.warn("task_explorer.count.failed", error);
        this.value = null;
      }
    }
  }
}
