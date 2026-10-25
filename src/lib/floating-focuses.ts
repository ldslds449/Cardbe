import { invoke } from "@tauri-apps/api/core";
import { parseCommandError } from "#lib/command-errors.js";
import { logger } from "#lib/logger.js";

// Like floating tasks, floating Focuses are a device-local convenience
// kept in WebView storage. Window position and size are restored by the
// window-state plugin per label.
const STORAGE_KEY = "cardbe.floating-focuses";

export function read_floating_focuses(): string[] {
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

function write_floating_focuses(ids: string[]) {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
  } catch (error) {
    logger.warn("floating_focus.storage_write.failed", error);
  }
}

export function forget_floating_focus(id: string) {
  write_floating_focuses(
    read_floating_focuses().filter((candidate) => candidate !== id),
  );
}

/** Opens (or focuses) the floating window and remembers it for next launch. */
export async function open_floating_focus(id: string) {
  await invoke("open_floating_focus", { id });
  const ids = read_floating_focuses();
  if (!ids.includes(id)) {
    write_floating_focuses([...ids, id]);
  }
}

/** Reopens remembered windows, dropping views that no longer exist. */
export async function restore_floating_focuses() {
  for (const id of read_floating_focuses()) {
    try {
      await invoke("open_floating_focus", { id });
    } catch (error) {
      if (parseCommandError(error)?.code === "FOCUS_NOT_FOUND") {
        forget_floating_focus(id);
      } else {
        logger.warn("floating_focus.restore.failed", error);
      }
    }
  }
}

/** Reads the view ID that the backend encodes as UTF-8 hex in the URL. */
export function decode_floating_focus_id(hex: string | null) {
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
