import { expect, it } from "vite-plus/test";
import {
  createVirtualizer,
  type SvelteVirtualizer,
} from "@tanstack/svelte-virtual";

it("updates fixed-height list geometry and stable keys after append and reorder", () => {
  const list = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => null,
    estimateSize: () => 72,
  });
  const unsubscribe = list.subscribe((virtualizer) => {
    expect(virtualizer.getTotalSize()).toBe(virtualizer.options.count * 72);
  });
  let instance!: SvelteVirtualizer<HTMLDivElement, HTMLDivElement>;
  const stop = list.subscribe((value) => (instance = value));
  const update = (keys: string[]) =>
    instance.setOptions({
      count: keys.length,
      getItemKey: (index) => keys[index],
    });
  update(["board-1-task-1", "board-2-task-1"]);
  expect(instance.getTotalSize()).toBe(144);
  update(["board-2-task-1", "board-1-task-1", "board-1-task-1-archived"]);
  expect([0, 1, 2].map((index) => instance.options.getItemKey(index))).toEqual([
    "board-2-task-1",
    "board-1-task-1",
    "board-1-task-1-archived",
  ]);
  expect(instance.getTotalSize()).toBe(216);
  update([]);
  expect(instance.getTotalSize()).toBe(0);
  stop();
  unsubscribe();
});
