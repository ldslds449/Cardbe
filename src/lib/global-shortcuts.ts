// Mirrors the backend rules in `src-tauri/src/global_shortcuts.rs`, which stays
// the authority: Ctrl or Alt is required and keys are A-Z, 0-9 or F1-F24.

export type ShortcutAction =
  | "quick_task"
  | "quick_note"
  | "floating_tasks"
  | "show_cardbe";

export interface GlobalShortcutStatus {
  action: ShortcutAction;
  shortcut: string | null;
  default_shortcut: string;
  /** False when global shortcuts are off or another app owns the keys. */
  active: boolean;
}

export interface GlobalShortcutOverview {
  enabled: boolean;
  shortcuts: GlobalShortcutStatus[];
}

export const GLOBAL_SHORTCUTS_CHANGED_EVENT = "cardbe:global-shortcuts-changed";

export type ShortcutKeydown =
  | { kind: "shortcut"; shortcut: string }
  | { kind: "incomplete" }
  | { kind: "modifier" };

function key_name(code: string): string | null {
  const match =
    code.match(/^Key([A-Z])$/) ??
    code.match(/^Digit(\d)$/) ??
    code.match(/^(F(?:[1-9]|1\d|2[0-4]))$/);
  return match?.[1] ?? null;
}

type KeydownLike = Pick<
  KeyboardEvent,
  "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

/** Modifiers held during recording, in canonical order, for a live preview. */
export function held_modifiers(
  event: Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey">,
): string[] {
  return [
    event.ctrlKey && "Ctrl",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
  ].filter((key): key is string => Boolean(key));
}

export function shortcut_from_keydown(event: KeydownLike): ShortcutKeydown {
  if (/^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(event.code)) {
    return { kind: "incomplete" };
  }
  const key = key_name(event.code);
  if (!key || event.metaKey || !(event.ctrlKey || event.altKey)) {
    return { kind: "modifier" };
  }
  return {
    kind: "shortcut",
    shortcut: [...held_modifiers(event), key].join("+"),
  };
}

/** Human-readable form used in visible text, e.g. `Ctrl + Alt + T`. */
export function display_shortcut(shortcut: string): string {
  return shortcut.split("+").join(" + ");
}
