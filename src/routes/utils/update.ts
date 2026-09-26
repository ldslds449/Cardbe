import updateConfig from "../../config/update.json";

export const UPDATE_STARTUP_DELAY_MS = updateConfig.startupDelayMs;
export const UPDATE_CHECK_INTERVAL_MS =
  updateConfig.checkIntervalHours * 60 * 60 * 1000;
export const UPDATE_LAST_CHECK_KEY = "app.update.lastSuccessfulCheck";

export function updateCheckErrorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error.trim();
  if (error instanceof Error && error.message.trim())
    return error.message.trim();

  if (error && typeof error === "object" && "message" in error) {
    const message = Reflect.get(error, "message");
    if (typeof message === "string" && message.trim()) return message.trim();
  }

  return "An unknown error occurred.";
}

export function shouldCheckForUpdate(
  storage: Pick<Storage, "getItem">,
  now = Date.now(),
): boolean {
  const value = storage.getItem(UPDATE_LAST_CHECK_KEY);
  if (value === null) return true;

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
