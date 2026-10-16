import { logger } from "$lib/logger";

const storageKey = "cardbe-task-expansion";

type ExpandedTasks = Record<string, string[]>;

function readStored(): ExpandedTasks {
  try {
    const parsed: unknown = JSON.parse(
      globalThis.localStorage?.getItem(storageKey) ?? "{}",
    );
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return {};
    }
    const restored: ExpandedTasks = {};
    for (const [board_id, task_ids] of Object.entries(parsed)) {
      if (Array.isArray(task_ids)) {
        const valid = task_ids.filter((id) => typeof id === "string");
        if (valid.length > 0) {
          restored[board_id] = valid;
        }
      }
    }
    return restored;
  } catch {
    // Storage may be unavailable or corrupt; start with collapsed tasks.
    return {};
  }
}

// Tracks tasks individually expanded while the board-wide mode hides details.
// This is a device-local view preference, so it stays out of SQLite and Undo.
export class TaskExpansionState {
  private expanded = $state<ExpandedTasks>(readStored());

  is_expanded(board_id: number | null, task_id: string): boolean {
    return (
      board_id !== null && (this.expanded[board_id]?.includes(task_id) ?? false)
    );
  }

  toggle(board_id: number | null, task_id: string) {
    if (board_id === null) {
      return;
    }
    const current = this.expanded[board_id] ?? [];
    const next = current.includes(task_id)
      ? current.filter((id) => id !== task_id)
      : [...current, task_id];
    this.set_board(board_id, next);
    this.persist();
  }

  // Drop entries for tasks no longer on the board (deleted, archived, moved).
  prune(board_id: number | null, task_ids: string[]) {
    if (board_id === null || !this.expanded[board_id]) {
      return;
    }
    const present = new Set(task_ids);
    const current = this.expanded[board_id];
    const next = current.filter((id) => present.has(id));
    if (next.length !== current.length) {
      this.set_board(board_id, next);
      this.persist();
    }
  }

  clear() {
    if (Object.keys(this.expanded).length === 0) {
      return;
    }
    this.expanded = {};
    this.persist();
  }

  private set_board(board_id: number, task_ids: string[]) {
    if (task_ids.length > 0) {
      this.expanded[board_id] = task_ids;
    } else {
      delete this.expanded[board_id];
    }
  }

  private persist() {
    try {
      globalThis.localStorage?.setItem(
        storageKey,
        JSON.stringify(this.expanded),
      );
    } catch (error) {
      logger.warn("settings.task_expansion_cache.failed", error);
    }
  }
}
