import { describe, expect, it, vi } from "vitest";
import { LatestShareSyncQueue } from "./share-sync-queue";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolve_fn, reject_fn) => {
    resolve = resolve_fn;
    reject = reject_fn;
  });
  return { promise, resolve, reject };
}

describe("LatestShareSyncQueue", () => {
  it("waits for a request queued behind an in-flight publish", async () => {
    const first = deferred<void>();
    const run = vi.fn(async (_id: string, value: string) => {
      if (value === "first") await first.promise;
    });
    const queue = new LatestShareSyncQueue(run, vi.fn(), vi.fn());

    const initial = queue.request("share", "first");
    const queued = queue.request("share", "latest");
    await Promise.resolve();
    expect(run).toHaveBeenCalledWith("share", "first", expect.any(Function));

    first.resolve();
    await initial;
    await queued;
    expect(run).toHaveBeenLastCalledWith(
      "share",
      "latest",
      expect.any(Function),
    );
    expect(run).toHaveBeenCalledTimes(2);
  });

  it("keeps an error terminal when the latest request fails", async () => {
    const error = new Error("offline");
    const on_error = vi.fn();
    const queue = new LatestShareSyncQueue<string>(
      async () => {
        throw error;
      },
      vi.fn(),
      on_error,
    );

    await expect(queue.request("share", "latest")).rejects.toThrow("offline");
    expect(on_error).toHaveBeenCalledWith("share", error);
  });

  it("settles waiters and drops queued work when a share is retired", async () => {
    const first = deferred<void>();
    const run = vi.fn(async (_id: string, value: string) => {
      if (value === "first") await first.promise;
    });
    const queue = new LatestShareSyncQueue(run, vi.fn(), vi.fn());
    const initial = queue.request("share", "first");
    const queued = queue.request("share", "newer");
    await Promise.resolve();
    queue.retire("share");
    await queued;
    first.resolve();
    await initial;
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("marks a late run stale after retirement and recreation", async () => {
    const first = deferred<void>();
    let was_current = true;
    const queue = new LatestShareSyncQueue<string>(
      async (_id, value, is_current) => {
        if (value === "first") {
          await first.promise;
          was_current = is_current();
        }
      },
      vi.fn(),
      vi.fn(),
    );
    const initial = queue.request("share", "first");
    await Promise.resolve();
    queue.retire("share");
    const recreated = queue.request("share", "recreated");
    first.resolve();
    await Promise.all([initial, recreated]);
    expect(was_current).toBe(false);
  });

  it("ignores an old publish error after retirement and recreation", async () => {
    const first = deferred<void>();
    const old_error = new Error("old publish failed");
    const on_error = vi.fn();
    const queue = new LatestShareSyncQueue<string>(
      async (_id, value) => {
        if (value === "first") {
          await first.promise;
          throw old_error;
        }
      },
      vi.fn(),
      on_error,
    );

    const initial = queue.request("share", "first");
    await Promise.resolve();
    queue.retire("share");
    const recreated = queue.request("share", "recreated");
    first.resolve();

    await expect(initial).resolves.toBeUndefined();
    await expect(recreated).resolves.toBeUndefined();
    expect(on_error).not.toHaveBeenCalled();
  });
});
