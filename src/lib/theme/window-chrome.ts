import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { logger } from "$lib/logger";
import { themeAppearance, type ThemePreference } from "./themes";

type Rgb = [number, number, number];
let revision = 0;

// Canvas converts any CSS color syntax (including oklch tokens) to sRGB bytes.
function readToken(context: CanvasRenderingContext2D, token: string): Rgb {
  context.clearRect(0, 0, 1, 1);
  context.fillStyle = getComputedStyle(document.documentElement)
    .getPropertyValue(token)
    .trim();
  context.fillRect(0, 0, 1, 1);
  const [red, green, blue] = context.getImageData(0, 0, 1, 1).data;
  return [red, green, blue];
}

function captionColors() {
  const context = document
    .createElement("canvas")
    .getContext("2d", { willReadFrequently: true });
  return context
    ? {
        caption: readToken(context, "--background"),
        text: readToken(context, "--foreground"),
      }
    : null;
}

/** Matches the native title bar to the applied theme. */
export function syncWindowChrome(preference: ThemePreference) {
  if (!isTauri()) {
    return;
  }
  const current = ++revision;
  const appearance = themeAppearance(preference);
  // The window theme also drives the webview's prefers-color-scheme, so System
  // must stay unset to keep following the OS.
  getCurrentWindow()
    .setTheme(appearance === "system" ? null : appearance)
    .catch((error) => logger.warn("settings.window_theme.failed", error));
  // Read tokens after mode-watcher has applied the new theme attributes.
  requestAnimationFrame(() => {
    const colors = current === revision ? captionColors() : null;
    if (colors) {
      invoke("set_window_caption_color", colors).catch((error) =>
        logger.warn("settings.window_caption.failed", error),
      );
    }
  });
}
