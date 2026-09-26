import { describe, expect, it } from "vitest";

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
  it("preserves errors returned as strings by Tauri commands", () => {
    expect(updateCheckErrorMessage("GitHub returned 403 Forbidden")).toBe(
      "GitHub returned 403 Forbidden",
    );
  });

  it("extracts messages from Error instances and error-like objects", () => {
    expect(updateCheckErrorMessage(new Error("Request timed out"))).toBe(
      "Request timed out",
    );
    expect(updateCheckErrorMessage({ message: "Network unavailable" })).toBe(
      "Network unavailable",
    );
  });

  it("provides a useful fallback for empty or unrecognized errors", () => {
    expect(updateCheckErrorMessage("  ")).toBe("An unknown error occurred.");
    expect(updateCheckErrorMessage(null)).toBe("An unknown error occurred.");
  });
});
