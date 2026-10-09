import { browser } from "$app/environment";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { mode, setMode, setTheme } from "mode-watcher";
import { toast } from "svelte-sonner";
import { parseCommandError, translateCommandError } from "$lib/command-errors";
import { logger } from "$lib/logger";
import {
  isThemePreference,
  restoreTheme,
  resolveTheme,
  themeAppearance,
  type ThemePreference,
} from "./themes";

const cacheKey = "cardbe-theme";
let revision = 0;
function cachedPreference(): ThemePreference {
  if (browser) {
    try {
      return restoreTheme(
        localStorage.getItem(cacheKey),
        localStorage.getItem("mode-watcher-mode"),
      );
    } catch {
      // Storage may be unavailable; the backend remains authoritative.
    }
  }
  return "system";
}

export const themeState = $state({
  preference: cachedPreference(),
  ready: false,
  saving: false,
});

function cache(preference: ThemePreference) {
  try {
    localStorage.setItem(cacheKey, preference);
  } catch (error) {
    logger.warn("settings.theme_cache.failed", error);
  }
}

function applyTheme(preference: ThemePreference) {
  themeState.preference = preference;
  setMode(themeAppearance(preference));
  setTheme(resolveTheme(preference, mode.current ?? "light"));
}

export async function selectTheme(preference: ThemePreference) {
  if (
    !isThemePreference(preference) ||
    themeState.saving ||
    !themeState.ready
  ) {
    return;
  }
  const previous = themeState.preference;
  revision++;
  themeState.saving = true;
  applyTheme(preference);
  try {
    if (isTauri()) {
      await invoke("set_theme", { theme: preference });
    }
    cache(preference);
  } catch (error) {
    applyTheme(previous);
    toast.error(translateCommandError(parseCommandError(error)));
    themeState.saving = false;
    return;
  }
  themeState.saving = false;
  if (isTauri()) {
    try {
      await emit("cardbe:theme-changed", preference);
    } catch (error) {
      logger.warn("settings.theme_broadcast.failed", error);
    }
  }
}

export async function toggleTheme() {
  await selectTheme(mode.current === "dark" ? "light" : "dark");
}

export function initializeTheme() {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  applyTheme(themeState.preference);
  const refresh = async () => {
    if (themeState.saving) {
      return;
    }
    const currentRevision = ++revision;
    try {
      const settings = await invoke<{ theme?: unknown }>("get_settings");
      if (disposed || currentRevision !== revision || themeState.saving) {
        return;
      }
      const preference = restoreTheme(settings.theme, themeState.preference);
      if (settings.theme == null) {
        // Old installations kept appearance only in mode-watcher's local storage.
        await invoke("set_theme", { theme: preference });
        if (disposed || currentRevision !== revision || themeState.saving) {
          return;
        }
      }
      applyTheme(preference);
      cache(preference);
    } catch (error) {
      logger.warn("settings.theme_load.failed", error);
      if (!disposed) {
        toast.error(translateCommandError(parseCommandError(error)));
      }
    } finally {
      if (!disposed) {
        themeState.ready = true;
      }
    }
  };
  if (isTauri()) {
    void listen<unknown>("cardbe:theme-changed", (event) => {
      if (isThemePreference(event.payload)) {
        // Events can arrive out of order; always read the last committed preference.
        void refresh();
      }
    })
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
          void refresh();
        }
      })
      .catch((error) => {
        logger.warn("settings.theme_listen.failed", error);
        if (!disposed) {
          void refresh();
        }
      });
    window.addEventListener("focus", refresh);
  } else {
    themeState.ready = true;
  }
  return () => {
    disposed = true;
    unlisten?.();
    window.removeEventListener("focus", refresh);
  };
}
