import { invoke } from "@tauri-apps/api/core";
import { parseCommandError } from "$lib/command-errors";
import { logger } from "$lib/logger";

// Like floating tasks, floating smart views are a device-local convenience
// kept in WebView storage. Window position and size are restored by the
// window-state plugin per label.
const STORAGE_KEY = "cardbe.floating-smart-views";

export function read_floating_smart_views(): string[] {
  try {
    const parsed: unknown = JSON.parse(
      window.localStorage.getItem(STORAGE_KEY) ?? "[]",
    );
    return Array.isArray(parsed)
      ? [
          ...new Set(
            parsed.filter((id): id is string => typeof id === "string"),
          ),
        ]
      : [];
  } catch {
    return [];
  }
}

function write_floating_smart_views(ids: string[]) {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
  } catch (error) {
    logger.warn("floating_smart_view.storage_write.failed", error);
  }
}

export function forget_floating_smart_view(id: string) {
  write_floating_smart_views(
    read_floating_smart_views().filter((candidate) => candidate !== id),
  );
}

/** Opens (or focuses) the floating window and remembers it for next launch. */
export async function open_floating_smart_view(id: string) {
  await invoke("open_floating_smart_view", { id });
  const ids = read_floating_smart_views();
  if (!ids.includes(id)) {
    write_floating_smart_views([...ids, id]);
  }
}

/** Reopens remembered windows, dropping views that no longer exist. */
export async function restore_floating_smart_views() {
  for (const id of read_floating_smart_views()) {
    try {
      await invoke("open_floating_smart_view", { id });
    } catch (error) {
      if (parseCommandError(error)?.code === "SMART_VIEW_NOT_FOUND") {
        forget_floating_smart_view(id);
      } else {
        logger.warn("floating_smart_view.restore.failed", error);
      }
    }
  }
}

/** Reads the view ID that the backend encodes as UTF-8 hex in the URL. */
export function decode_floating_smart_view_id(hex: string | null) {
  if (!hex || hex.length % 2 !== 0 || !/^[0-9a-f]+$/.test(hex)) {
    return null;
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index++) {
    bytes[index] = parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}
