import { invoke } from "@tauri-apps/api/core";
import { parseCommandError } from "#lib/command-errors.js";
import { logger } from "#lib/logger.js";

// Floating reminder windows are a device-local convenience: the list is kept
// in WebView storage, outside board data, synchronization, and Undo. Window
// position and size are restored by the window-state plugin per label.
const STORAGE_KEY = "cardbe.floating-tasks";

export interface FloatingTaskRef {
  boardId: number;
  taskId: number;
}

function valid_ref(value: unknown): value is FloatingTaskRef {
  return (
    value !== null &&
    typeof value === "object" &&
    Number.isSafeInteger((value as FloatingTaskRef).boardId) &&
    Number.isSafeInteger((value as FloatingTaskRef).taskId)
  );
}

function same_ref(left: FloatingTaskRef, right: FloatingTaskRef) {
  return left.boardId === right.boardId && left.taskId === right.taskId;
}

export function read_floating_tasks(): FloatingTaskRef[] {
  try {
    const parsed: unknown = JSON.parse(
      window.localStorage.getItem(STORAGE_KEY) ?? "[]",
    );
    return Array.isArray(parsed)
      ? parsed
          .filter(valid_ref)
          .map(({ boardId, taskId }) => ({ boardId, taskId }))
      : [];
  } catch {
    return [];
  }
}

function write_floating_tasks(refs: FloatingTaskRef[]) {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(refs));
  } catch (error) {
    logger.warn("floating_task.storage_write.failed", error);
  }
}

export function remember_floating_task(ref: FloatingTaskRef) {
  const refs = read_floating_tasks();
  if (!refs.some((candidate) => same_ref(candidate, ref))) {
    write_floating_tasks([...refs, ref]);
  }
}

export function forget_floating_task(ref: FloatingTaskRef) {
  write_floating_tasks(
    read_floating_tasks().filter((candidate) => !same_ref(candidate, ref)),
  );
  try {
    window.localStorage.removeItem(zoom_key(ref));
    window.localStorage.removeItem(height_key(ref));
    window.localStorage.removeItem(collapsed_key(ref));
  } catch {
    // Window preferences are conveniences; stale values are harmless.
  }
}

const collapsed_key = (ref: FloatingTaskRef) =>
  `cardbe.floating-task-collapsed.${ref.boardId}.${ref.taskId}`;

export function read_floating_collapsed(ref: FloatingTaskRef) {
  try {
    return window.localStorage.getItem(collapsed_key(ref)) === "true";
  } catch {
    return false;
  }
}

export function write_floating_collapsed(
  ref: FloatingTaskRef,
  collapsed: boolean,
) {
  try {
    if (collapsed) {
      window.localStorage.setItem(collapsed_key(ref), "true");
    } else {
      window.localStorage.removeItem(collapsed_key(ref));
    }
  } catch (error) {
    logger.warn("floating_task.collapsed_write.failed", error);
  }
}

export const FLOATING_ZOOM_MIN = 0.6;
export const FLOATING_ZOOM_MAX = 2;

const zoom_key = (ref: FloatingTaskRef) =>
  `cardbe.floating-task-zoom.${ref.boardId}.${ref.taskId}`;

/** Rounds to 10% steps inside the supported range. */
export function clamp_floating_zoom(value: number) {
  const rounded = Math.round(value * 10) / 10;
  return Math.min(FLOATING_ZOOM_MAX, Math.max(FLOATING_ZOOM_MIN, rounded));
}

export function read_floating_zoom(ref: FloatingTaskRef) {
  try {
    const stored = Number(window.localStorage.getItem(zoom_key(ref)));
    return Number.isFinite(stored) && stored > 0
      ? clamp_floating_zoom(stored)
      : 1;
  } catch {
    return 1;
  }
}

export function write_floating_zoom(ref: FloatingTaskRef, zoom: number) {
  try {
    if (zoom === 1) {
      window.localStorage.removeItem(zoom_key(ref));
    } else {
      window.localStorage.setItem(zoom_key(ref), String(zoom));
    }
  } catch (error) {
    logger.warn("floating_task.zoom_write.failed", error);
  }
}

const height_key = (ref: FloatingTaskRef) =>
  `cardbe.floating-task-height.${ref.boardId}.${ref.taskId}`;

/** The height the user shortened the window to, or null to fit the content. */
export function read_floating_height(ref: FloatingTaskRef) {
  try {
    const stored = Number(window.localStorage.getItem(height_key(ref)));
    return Number.isFinite(stored) && stored > 0 ? stored : null;
  } catch {
    return null;
  }
}

export function write_floating_height(
  ref: FloatingTaskRef,
  height: number | null,
) {
  try {
    if (height === null) {
      window.localStorage.removeItem(height_key(ref));
    } else {
      window.localStorage.setItem(height_key(ref), String(height));
    }
  } catch (error) {
    logger.warn("floating_task.height_write.failed", error);
  }
}

/** Opens (or focuses) the floating window and remembers it for next launch. */
export async function open_floating_task(ref: FloatingTaskRef) {
  await invoke("open_floating_task", { ...ref });
  remember_floating_task(ref);
}

/** Reopens remembered windows, dropping tasks that no longer exist. */
export async function restore_floating_tasks() {
  for (const ref of read_floating_tasks()) {
    try {
      await invoke("open_floating_task", { ...ref });
    } catch (error) {
      const code = parseCommandError(error)?.code;
      if (code === "TASK_NOT_FOUND" || code === "BOARD_NOT_FOUND") {
        forget_floating_task(ref);
      } else {
        logger.warn("floating_task.restore.failed", error);
      }
    }
  }
}
