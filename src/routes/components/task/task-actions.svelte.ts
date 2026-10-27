import { translateCommandError } from "#lib/command-errors.js";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";

import type { AllTaskItem } from "../../board-base.svelte";
import {
  deserialize_column,
  get_column_id,
  type Column,
  type ColumnSerialized,
} from "../../type/column.svelte";
import { get_task_id, type TaskPriority } from "../../type/task.svelte";

/** Identifies a listed task; archived copies can share a task's ID. */
export function task_item_key(item: AllTaskItem): string {
  return `${item.board_id}:${item.archived_at ? "a" : ""}${item.task.id}`;
}

/**
 * Task list actions that write any board directly, without opening it. The
 * list mounts a column picker bound to `reopen_*` for reopening.
 */
export class TaskActions {
  busy = $state(new Set<string>());
  // Reopening asks where the task goes, among its board's open columns.
  reopen_item = $state<AllTaskItem | null>(null);
  reopen_columns = $state<Column[]>([]);
  reopen_open = $state(false);

  /** Runs after each action, such as reloading the list. */
  #after: () => void;

  constructor(after: () => void) {
    this.#after = after;
  }

  is_busy(item: AllTaskItem): boolean {
    return this.busy.has(task_item_key(item));
  }

  /** Runs `action` once at a time per task, reporting failures. */
  async run(item: AllTaskItem, action: () => Promise<unknown>, reload = true) {
    const key = task_item_key(item);
    if (this.busy.has(key)) {
      return;
    }
    this.busy = new Set(this.busy).add(key);
    try {
      await action();
    } catch (error) {
      toast.error(translateCommandError(error));
    } finally {
      const next = new Set(this.busy);
      next.delete(key);
      this.busy = next;
      if (reload) {
        this.#after();
      }
    }
  }

  complete(item: AllTaskItem) {
    void this.run(item, () =>
      invoke("complete_board_tasks", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
      }),
    );
  }

  set_priority(item: AllTaskItem, priority: TaskPriority | undefined) {
    void this.run(item, () =>
      invoke("set_board_tasks_priority", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
        priority: priority ?? null,
      }),
    );
  }

  /** Done and archived tasks both return to an open column. */
  ask_to_reopen(item: AllTaskItem) {
    void this.run(
      item,
      async () => {
        const columns = await invoke<ColumnSerialized[]>("get_board_columns", {
          boardId: item.board_id,
        });
        this.reopen_columns = columns
          .map(deserialize_column)
          .filter((column) => !column.done);
        this.reopen_item = item;
        this.reopen_open = true;
      },
      false,
    );
  }

  reopen(column_id: string) {
    const item = this.reopen_item;
    if (!item) {
      return;
    }
    void this.run(item, () =>
      invoke("reopen_board_tasks", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
        columnId: get_column_id(column_id),
      }),
    );
  }
}
