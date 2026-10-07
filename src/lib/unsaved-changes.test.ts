import { afterEach, expect, test, vi } from "vite-plus/test";
import { create_note_queue } from "../routes/components/note/note-queue";
import {
  confirmUnsavedChanges,
  hasUnsavedChanges,
  registerUnsavedChanges,
  setUnsavedChangesPrompt,
} from "./unsaved-changes";

afterEach(() => setUnsavedChangesPrompt());

test("only changed drafts prompt; cancelling preserves edits and saving or reverting skips the prompt", async () => {
  const initial = JSON.stringify({ title: "Task", items: [] });
  let current = initial;
  let open = true;
  const unregister = registerUnsavedChanges(() => open && current !== initial);
  const prompt = vi.fn(async () => false);
  setUnsavedChangesPrompt(prompt);
  try {
    expect(await confirmUnsavedChanges()).toBe(true);
    expect(prompt).not.toHaveBeenCalled();
    current = JSON.stringify({
      title: "Changed",
      items: [{ text: "Item", completed: true }],
    });
    expect(await confirmUnsavedChanges()).toBe(false);
    expect(hasUnsavedChanges()).toBe(true);
    prompt.mockResolvedValue(true);
    expect(await confirmUnsavedChanges()).toBe(true);
    current = initial;
    expect(await confirmUnsavedChanges()).toBe(true);
    expect(prompt).toHaveBeenCalledTimes(2);
    current = "saved";
    open = false;
    expect(hasUnsavedChanges()).toBe(false);
  } finally {
    unregister();
  }
  expect(hasUnsavedChanges()).toBe(false);
});

test("a dirty draft is preserved when the confirmation UI is unavailable", async () => {
  expect(await confirmUnsavedChanges(() => true)).toBe(false);
});

test("closing waits for autosave; a failed save remains eligible for confirmation", async () => {
  const enqueue = create_note_queue();
  let dirty = true;
  const unregister = registerUnsavedChanges(() => dirty);
  const prompt = vi.fn(async () => false);
  setUnsavedChangesPrompt(prompt);
  try {
    const failed = enqueue(async () => {
      throw new Error("save failed");
    });
    await expect(failed).rejects.toThrow("save failed");
    await enqueue(async () => undefined);
    expect(await confirmUnsavedChanges()).toBe(false);
    expect(dirty).toBe(true);
    void enqueue(async () => {
      dirty = false;
    });
    await enqueue(async () => undefined);
    expect(await confirmUnsavedChanges()).toBe(true);
    expect(prompt).toHaveBeenCalledOnce();
  } finally {
    unregister();
  }
});
