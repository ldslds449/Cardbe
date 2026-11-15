/** Apple desktops use ⌘ where other platforms use Ctrl for app shortcuts. */
export const is_apple_platform =
  typeof navigator !== "undefined" &&
  /Macintosh|Mac OS X/.test(navigator.userAgent);

/** Displayed name of the primary command modifier. */
export const primary_modifier_label = is_apple_platform ? "⌘" : "Ctrl";

/** `aria-keyshortcuts` name of the primary command modifier. */
export const primary_modifier_aria = is_apple_platform ? "Meta" : "Control";

/**
 * A compact shortcut label, such as `⌘K` or `⌘⇧F` on macOS and `Ctrl+K` or
 * `Ctrl+Shift+F` elsewhere.
 */
export function primary_shortcut_label(key: string, shift = false): string {
  return is_apple_platform
    ? `⌘${shift ? "⇧" : ""}${key}`
    : `Ctrl+${shift ? "Shift+" : ""}${key}`;
}

/**
 * Whether exactly the platform's primary command modifier is held: ⌘ without
 * Control on macOS, Ctrl without the Windows key elsewhere.
 */
export function has_primary_modifier(
  event: Pick<KeyboardEvent, "ctrlKey" | "metaKey">,
  apple = is_apple_platform,
): boolean {
  return apple
    ? event.metaKey && !event.ctrlKey
    : event.ctrlKey && !event.metaKey;
}
