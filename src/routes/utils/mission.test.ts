import { describe, expect, it, vi } from "vite-plus/test";

const logger = vi.hoisted(() => ({ error: vi.fn() }));
vi.mock("#lib/logger.js", () => ({ logger }));

import { addMission } from "./mission.svelte";

describe("mission queue", () => {
  it("returns the original error and continues with the next mission", async () => {
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
    expect(logger.error).toHaveBeenCalledWith("mission.failed", failure);
  });
});
