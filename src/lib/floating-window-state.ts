import { invoke } from "@tauri-apps/api/core";
import type { Window } from "@tauri-apps/api/window";
import { logger } from "#lib/logger.js";

/**
 * Writes the window-state plugin's cache to disk. Its own Moved and Resized
 * handlers already keep positions and sizes current, so this passes no flags:
 * with flags, the plugin holds its cache lock while querying every window on
 * the main thread, and a Moved or Resized event there waits for the same
 * lock, freezing the whole app during a drag or resize.
 */
export function save_window_state() {
  return invoke("plugin:window-state|save_window_state", { flags: 0 });
}

export async function persist_floating_window_position(
  float_window: Pick<Window, "onMoved">,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const unlisten = await float_window.onMoved(() => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      void save_window_state().catch((error) =>
        logger.warn("floating_window.position_write.failed", error),
      );
    }, 250);
  });
  return () => {
    clearTimeout(timer);
    unlisten();
  };
}
