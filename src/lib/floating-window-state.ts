import { invoke } from "@tauri-apps/api/core";
import type { Window } from "@tauri-apps/api/window";
import { logger } from "#lib/logger.js";

export async function persist_floating_window_position(
  float_window: Pick<Window, "onMoved">,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const unlisten = await float_window.onMoved(() => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      void invoke("plugin:window-state|save_window_state").catch((error) =>
        logger.warn("floating_window.position_write.failed", error),
      );
    }, 250);
  });
  return () => {
    clearTimeout(timer);
    unlisten();
  };
}
