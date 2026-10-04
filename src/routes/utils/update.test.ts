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
  it("localizes structured service errors without parsing backend wording", () => {
    expect(updateCheckErrorMessage({ code: "UPDATE_REQUEST_FAILED" })).toBe(
      "Could not contact GitHub. Check your connection and try again.",
    );
    expect(
      updateCheckErrorMessage({ code: "UPDATE_RELEASE_DATA_INVALID" }),
    ).toBe("GitHub returned invalid release information.");
  });
  it("hides unknown values and legacy backend messages", () => {
    for (const error of [
      null,
      "GitHub returned 403 Forbidden",
      new Error("Request timed out"),
      { message: "Network unavailable" },
    ]) {
      expect(updateCheckErrorMessage(error)).toBe(
        "Something went wrong. Please try again.",
      );
    }
  });
});
