import { invoke } from "@tauri-apps/api/core";
import { parseCommandError } from "#lib/command-errors.js";
import { logger } from "#lib/logger.js";
import {
  forget_floating_task,
  open_floating_task,
  read_floating_tasks,
} from "#lib/floating-tasks.js";
import {
  forget_floating_focus,
  open_floating_focus,
  read_floating_focuses,
} from "#lib/floating-focuses.js";

export type DesktopWidgetRef =
  | { kind: "task"; boardId: number; taskId: number }
  | { kind: "focus"; id: string };

/** Desktop widgets attach to the desktop; floating ones stay on top. */
export type WidgetPlacement = "desktop" | "floating";

export const desktop_widgets = $state({
  supported: false,
  /** The source the widget settings should present when they open next. */
  requested: null as DesktopWidgetRef | null,
});

export const DESKTOP_WIDGETS_STORAGE_KEY = "cardbe.desktop-widgets";

function valid(value: unknown): value is DesktopWidgetRef {
  if (!value || typeof value !== "object") {
    return false;
  }
  const ref = value as DesktopWidgetRef;
  return ref.kind === "focus"
    ? typeof ref.id === "string"
    : ref.kind === "task" &&
        Number.isSafeInteger(ref.boardId) &&
        Number.isSafeInteger(ref.taskId);
}

function same(left: DesktopWidgetRef, right: DesktopWidgetRef) {
  return left.kind === "focus" && right.kind === "focus"
    ? left.id === right.id
    : left.kind === "task" &&
        right.kind === "task" &&
        left.boardId === right.boardId &&
        left.taskId === right.taskId;
}

export function read_desktop_widgets(): DesktopWidgetRef[] {
  try {
    const refs: unknown = JSON.parse(
      window.localStorage.getItem(DESKTOP_WIDGETS_STORAGE_KEY) ?? "[]",
    );
    return Array.isArray(refs)
      ? refs
          .filter(valid)
          .map((ref): DesktopWidgetRef =>
            ref.kind === "focus"
              ? { kind: "focus", id: ref.id }
              : { kind: "task", boardId: ref.boardId, taskId: ref.taskId },
          )
          .filter(
            (ref, index, items) =>
              items.findIndex((candidate) => same(candidate, ref)) === index,
          )
      : [];
  } catch {
    return [];
  }
}

function write(refs: DesktopWidgetRef[]) {
  try {
    window.localStorage.setItem(
      DESKTOP_WIDGETS_STORAGE_KEY,
      JSON.stringify(refs),
    );
  } catch (error) {
    logger.warn("desktop_widget.preference_write.failed", error);
  }
}

export function forget_desktop_widget(ref: DesktopWidgetRef) {
  write(read_desktop_widgets().filter((candidate) => !same(candidate, ref)));
}

function open(ref: DesktopWidgetRef) {
  return ref.kind === "task"
    ? invoke("open_desktop_task", { boardId: ref.boardId, taskId: ref.taskId })
    : invoke("open_desktop_focus", { id: ref.id });
}

export async function open_desktop_widget(ref: DesktopWidgetRef) {
  await open(ref);
  const refs = read_desktop_widgets();
  if (!refs.some((candidate) => same(candidate, ref))) {
    write([...refs, ref]);
  }
}

export async function remove_desktop_widget(ref: DesktopWidgetRef) {
  await invoke("remove_widget", { widget: ref, placement: "desktop" });
  forget_desktop_widget(ref);
}

/** Remembered widgets of one placement, in the order they were added. */
export function read_widgets(placement: WidgetPlacement): DesktopWidgetRef[] {
  return placement === "desktop"
    ? read_desktop_widgets()
    : [
        ...read_floating_focuses().map((id): DesktopWidgetRef => ({
          kind: "focus",
          id,
        })),
        ...read_floating_tasks().map((ref): DesktopWidgetRef => ({
          kind: "task",
          ...ref,
        })),
      ];
}

export function open_widget(ref: DesktopWidgetRef, placement: WidgetPlacement) {
  if (placement === "desktop") {
    return open_desktop_widget(ref);
  }
  return ref.kind === "focus"
    ? open_floating_focus(ref.id)
    : open_floating_task({ boardId: ref.boardId, taskId: ref.taskId });
}

export async function remove_widget(
  ref: DesktopWidgetRef,
  placement: WidgetPlacement,
) {
  if (placement === "desktop") {
    return remove_desktop_widget(ref);
  }
  await invoke("remove_widget", { widget: ref, placement });
  if (ref.kind === "focus") {
    forget_floating_focus(ref.id);
  } else {
    forget_floating_task({ boardId: ref.boardId, taskId: ref.taskId });
  }
}

/** Whether each floating window is open and visible. */
export function read_floating_visibility(refs: DesktopWidgetRef[]) {
  return invoke<boolean[]>("floating_windows_visible", { widgets: refs });
}

export function hide_floating_widget(ref: DesktopWidgetRef) {
  return invoke("hide_floating_window", { widget: ref });
}

export async function restore_desktop_widgets() {
  desktop_widgets.supported = await invoke<boolean>("desktop_widget_supported");
  if (!desktop_widgets.supported) {
    return;
  }
  for (const ref of read_desktop_widgets()) {
    try {
      await open(ref);
    } catch (error) {
      const code = parseCommandError(error)?.code;
      if (
        code === "TASK_NOT_FOUND" ||
        code === "BOARD_NOT_FOUND" ||
        code === "FOCUS_NOT_FOUND"
      ) {
        forget_desktop_widget(ref);
      } else {
        logger.warn("desktop_widget.restore.failed", error);
      }
    }
  }
}

/** Compact content applies below this logical width or height. */
export function compact_widget(width: number, height: number) {
  return width < 280 || height < 160;
}
