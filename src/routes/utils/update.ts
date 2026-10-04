import * as m from "$lib/paraglide/messages.js";
import updateConfig from "../../config/update.json";

export const UPDATE_STARTUP_DELAY_MS = updateConfig.startupDelayMs;
export const UPDATE_CHECK_INTERVAL_MS =
  updateConfig.checkIntervalHours * 60 * 60 * 1000;
export const UPDATE_LAST_CHECK_KEY = "app.update.lastSuccessfulCheck";

export function updateCheckErrorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) {
    return localizedUpdateError(error.trim());
  }
  if (error instanceof Error && error.message.trim()) {
    return localizedUpdateError(error.message.trim());
  }

  if (error && typeof error === "object" && "message" in error) {
    const message = Reflect.get(error, "message");
    if (typeof message === "string" && message.trim()) {
      return localizedUpdateError(message.trim());
    }
  }

  return m.common_unknown_error();
}

function localizedUpdateError(message: string): string {
  const status = /^GitHub returned (\d{3})\b/.exec(message);
  if (status) {
    return m.update_http_error({ status: status[1] });
  }
  if (message.startsWith("GitHub returned invalid release data")) {
    return m.update_release_data_error();
  }
  if (/contact GitHub|network|timed out|fetch failed/i.test(message)) {
    return m.update_network_error();
  }
  return m.common_unknown_error();
}

export function shouldCheckForUpdate(
  storage: Pick<Storage, "getItem">,
  now = Date.now(),
): boolean {
  const value = storage.getItem(UPDATE_LAST_CHECK_KEY);
  if (value === null) {
    return true;
  }

  const lastCheck = Number(value);
  return (
    !Number.isFinite(lastCheck) || now - lastCheck >= UPDATE_CHECK_INTERVAL_MS
  );
}

export function markUpdateCheckSuccessful(
  storage: Pick<Storage, "setItem">,
  now = Date.now(),
): void {
  storage.setItem(UPDATE_LAST_CHECK_KEY, String(now));
}
