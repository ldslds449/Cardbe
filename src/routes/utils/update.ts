import { translateCommandError } from "$lib/command-errors";
import updateConfig from "../../config/update.json";

export const UPDATE_STARTUP_DELAY_MS = updateConfig.startupDelayMs;
export const UPDATE_CHECK_INTERVAL_MS =
  updateConfig.checkIntervalHours * 60 * 60 * 1000;
// Long-running sessions re-evaluate the check interval periodically, which
// also retries failed checks without waiting for the next launch.
export const UPDATE_POLL_INTERVAL_MS =
  updateConfig.pollIntervalMinutes * 60 * 1000;
export const UPDATE_LAST_CHECK_KEY = "app.update.lastSuccessfulCheck";

export const updateCheckErrorMessage = translateCommandError;

export function shouldCheckForUpdate(
  storage: Pick<Storage, "getItem">,
  now = Date.now(),
): boolean {
  const value = storage.getItem(UPDATE_LAST_CHECK_KEY);
  if (value === null) {
    return true;
  }

  const lastCheck = Number(value);
  // A timestamp in the future means the system clock moved backwards.
  return (
    !Number.isFinite(lastCheck) ||
    lastCheck > now ||
    now - lastCheck >= UPDATE_CHECK_INTERVAL_MS
  );
}

export function markUpdateCheckSuccessful(
  storage: Pick<Storage, "setItem">,
  now = Date.now(),
): void {
  storage.setItem(UPDATE_LAST_CHECK_KEY, String(now));
}
