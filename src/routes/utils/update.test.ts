import { describe, expect, it } from "vite-plus/test";

import {
  UPDATE_CHECK_INTERVAL_MS,
  UPDATE_LAST_CHECK_KEY,
  markUpdateCheckSuccessful,
  shouldCheckForUpdate,
  updateCheckErrorMessage,
} from "./update";

describe("update check interval", () => {
  it("waits 24 hours after a successful check", () => {
    const values = new Map<string, string>();
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
    };
    const now = 1_000_000;

    expect(shouldCheckForUpdate(storage, now)).toBe(true);
    markUpdateCheckSuccessful(storage, now);
    expect(values.get(UPDATE_LAST_CHECK_KEY)).toBe(String(now));
    expect(
      shouldCheckForUpdate(storage, now + UPDATE_CHECK_INTERVAL_MS - 1),
    ).toBe(false);
    expect(shouldCheckForUpdate(storage, now + UPDATE_CHECK_INTERVAL_MS)).toBe(
      true,
    );
  });
});

describe("update check errors", () => {
  it("localizes HTTP errors returned by Tauri commands", () => {
    expect(updateCheckErrorMessage("GitHub returned 403 Forbidden")).toBe(
      "GitHub returned HTTP 403. Try again later.",
    );
  });

  it("localizes errors from Error instances and error-like objects", () => {
    expect(updateCheckErrorMessage(new Error("Request timed out"))).toBe(
      "Could not contact GitHub. Check your connection and try again.",
    );
    expect(updateCheckErrorMessage({ message: "Network unavailable" })).toBe(
      "Could not contact GitHub. Check your connection and try again.",
    );
  });

  it("provides a useful fallback for empty or unrecognized errors", () => {
    expect(updateCheckErrorMessage("  ")).toBe("An unknown error occurred.");
    expect(updateCheckErrorMessage(null)).toBe("An unknown error occurred.");
    expect(updateCheckErrorMessage("Unexpected internal failure")).toBe(
      "An unknown error occurred.",
    );
    expect(
      updateCheckErrorMessage("GitHub returned invalid release data: EOF"),
    ).toBe("GitHub returned invalid release information.");
  });
});
