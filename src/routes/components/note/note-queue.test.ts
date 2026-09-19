import { describe, expect, it, vi } from "vitest";
import { create_note_queue } from "./note-queue";

describe("note persistence queue", () => {
  it("waits for an older save before writing new content or deleting", async () => {
    const enqueue = create_note_queue();
    let finish!: () => void;
    const first_write = new Promise<void>((resolve) => { finish = resolve; });
    let stored: string | null = "initial";
    const first = enqueue(async () => {
      await first_write;
      stored = "old edit";
    });
    const update = vi.fn(async () => { stored = "new edit"; });
    const second = enqueue(update);
    const remove = vi.fn(async () => { stored = null; });
    const deleted = enqueue(remove);

    await Promise.resolve();
    expect(update).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
    finish();
    await Promise.all([first, second, deleted]);
    expect(update).toHaveBeenCalledOnce();
    expect(remove).toHaveBeenCalledOnce();
    expect(stored).toBeNull();
  });

  it("reports a failed write and still permits a subsequent retry", async () => {
    const enqueue = create_note_queue();
    const failed = enqueue(async () => { throw new Error("disk unavailable"); });
    const retry = enqueue(async () => "saved");
    await expect(failed).rejects.toThrow("disk unavailable");
    await expect(retry).resolves.toBe("saved");
  });
});
