import { describe, expect, it, vi } from "vitest";

import { addMission } from "./mission.svelte";

describe("mission queue", () => {
  it("returns the original error and continues with the next mission", async () => {
    const console_error = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});
    const calls: string[] = [];
    const failure = new Error("save failed");

    const failed = addMission(async () => {
      calls.push("failed");
      throw failure;
    });
    const succeeded = addMission(async () => {
      calls.push("succeeded");
      return "done";
    });

    await expect(failed).rejects.toBe(failure);
    await expect(succeeded).resolves.toBe("done");
    expect(calls).toEqual(["failed", "succeeded"]);
    expect(console_error).toHaveBeenCalledWith("Mission error:", failure);

    console_error.mockRestore();
  });
});
