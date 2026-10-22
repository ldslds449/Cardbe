import { logger } from "$lib/logger";

const storageKey = "cardbe-layout-widths";

export type WidthBounds = { min: number; max: number; default: number };

export const column_width_bounds: WidthBounds = {
  min: 240,
  max: 640,
  default: 320,
};
export const view_dialog_width_bounds: WidthBounds = {
  min: 400,
  max: 1600,
  default: 960,
};
export const edit_dialog_width_bounds: WidthBounds = {
  min: 400,
  max: 1600,
  default: 960,
};

// Task dialogs at least this wide show task details beside the description
// instead of above it, so long descriptions use the horizontal space.
export const task_dialog_split_width = 800;

// Dialogs are capped at the viewport width minus their 1rem margins.
export function task_dialog_is_split(width: number, viewport_width: number) {
  return Math.min(width, viewport_width - 32) >= task_dialog_split_width;
}

type StoredWidths = {
  columns: Record<string, number>;
  view_dialog?: number;
  edit_dialog?: number;
};

export function clamp_width(width: number, bounds: WidthBounds): number {
  if (!Number.isFinite(width)) {
    return bounds.default;
  }
  return Math.round(Math.min(bounds.max, Math.max(bounds.min, width)));
}

function valid_width(value: unknown, bounds: WidthBounds): number | undefined {
  return typeof value === "number" &&
    Number.isFinite(value) &&
    value >= bounds.min &&
    value <= bounds.max
    ? Math.round(value)
    : undefined;
}

function readStored(): StoredWidths {
  try {
    const parsed: unknown = JSON.parse(
      globalThis.localStorage?.getItem(storageKey) ?? "{}",
    );
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { columns: {} };
    }
    const record = parsed as Record<string, unknown>;
    const columns: Record<string, number> = {};
    const stored_columns = record.columns;
    if (
      stored_columns &&
      typeof stored_columns === "object" &&
      !Array.isArray(stored_columns)
    ) {
      for (const [board_id, width] of Object.entries(stored_columns)) {
        const valid = valid_width(width, column_width_bounds);
        if (valid !== undefined) {
          columns[board_id] = valid;
        }
      }
    }
    return {
      columns,
      view_dialog: valid_width(record.view_dialog, view_dialog_width_bounds),
      edit_dialog: valid_width(record.edit_dialog, edit_dialog_width_bounds),
    };
  } catch {
    // Storage may be unavailable or corrupt; start with default widths.
    return { columns: {} };
  }
}

// Device-local layout widths: per-board column width and task dialog widths.
// These are view preferences, so they stay out of SQLite, sync, and Undo.
export class LayoutWidthState {
  private widths = $state<StoredWidths>(readStored());

  column_width(board_id: number | null): number {
    return (
      (board_id !== null ? this.widths.columns[board_id] : undefined) ??
      column_width_bounds.default
    );
  }

  set_column_width(board_id: number | null, width: number, persist = true) {
    if (board_id === null) {
      return;
    }
    const next = clamp_width(width, column_width_bounds);
    if (next === column_width_bounds.default) {
      delete this.widths.columns[board_id];
    } else {
      this.widths.columns[board_id] = next;
    }
    if (persist) {
      this.persist();
    }
  }

  get view_dialog_width(): number {
    return this.widths.view_dialog ?? view_dialog_width_bounds.default;
  }

  set_view_dialog_width(width: number, persist = true) {
    this.widths.view_dialog = clamp_width(width, view_dialog_width_bounds);
    if (persist) {
      this.persist();
    }
  }

  get edit_dialog_width(): number {
    return this.widths.edit_dialog ?? edit_dialog_width_bounds.default;
  }

  set_edit_dialog_width(width: number, persist = true) {
    this.widths.edit_dialog = clamp_width(width, edit_dialog_width_bounds);
    if (persist) {
      this.persist();
    }
  }

  // Drop column widths for boards that no longer exist.
  prune_boards(board_ids: number[]) {
    const present = new Set(board_ids.map(String));
    const stale = Object.keys(this.widths.columns).filter(
      (id) => !present.has(id),
    );
    if (stale.length === 0) {
      return;
    }
    for (const id of stale) {
      delete this.widths.columns[id];
    }
    this.persist();
  }

  private persist() {
    try {
      globalThis.localStorage?.setItem(storageKey, JSON.stringify(this.widths));
    } catch (error) {
      logger.warn("settings.layout_width_cache.failed", error);
    }
  }
}

export const layout_widths = new LayoutWidthState();
