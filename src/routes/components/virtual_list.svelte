<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import { untrack } from "svelte";
  import { createVirtualizer } from "@tanstack/svelte-virtual";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { cn } from "#lib/utils.js";

  let {
    items,
    getItemKey,
    rowHeight = 104,
    overscan = 6,
    hasMore = false,
    loading = false,
    resetKey = "",
    class: className = "",
    onLoadMore,
    children,
  }: {
    items: T[];
    getItemKey: (item: T) => string;
    rowHeight?: number;
    overscan?: number;
    hasMore?: boolean;
    loading?: boolean;
    resetKey?: string;
    class?: string;
    onLoadMore?: () => void;
    children: Snippet<[T, number]>;
  } = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let previous_reset_key = "";
  let measured_row_height = 104;
  const virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: 0,
    getScrollElement: () => viewport,
    estimateSize: () => 104,
  });

  $effect.pre(() => {
    const currentItems = items;
    const currentGetItemKey = getItemKey;
    const currentViewport = viewport;
    const currentRowHeight = rowHeight;
    const currentOverscan = overscan;
    untrack(() => {
      $virtualizer.setOptions({
        count: currentItems.length,
        getScrollElement: () => currentViewport,
        estimateSize: () => currentRowHeight,
        getItemKey: (index) => currentGetItemKey(currentItems[index]),
        overscan: currentOverscan,
      });
      // Row offsets are cached per key, so a new height recomputes them.
      if (currentRowHeight !== measured_row_height) {
        measured_row_height = currentRowHeight;
        $virtualizer.measure();
      }
    });
  });
  $effect(() => {
    if (resetKey === previous_reset_key) {
      return;
    }
    previous_reset_key = resetKey;
    if (viewport) {
      viewport.scrollTop = 0;
    }
    untrack(() => $virtualizer.scrollToOffset(0));
  });

  $effect(() => {
    if (
      hasMore &&
      !loading &&
      ($virtualizer.getVirtualItems().at(-1)?.index ?? -1) + 1 >=
        Math.max(0, items.length - overscan * 2)
    ) {
      onLoadMore?.();
    }
  });
</script>

<ScrollArea
  bind:viewport
  class={cn("h-full rounded-md", className)}
  scrollbarYClasses="mr-1"
  type="auto"
  orientation="vertical"
>
  <div class="relative" style={`height: ${$virtualizer.getTotalSize()}px;`}>
    <div
      class="absolute left-0 right-5 top-0"
      style={`transform: translateY(${$virtualizer.getVirtualItems()[0]?.start ?? 0}px);`}
    >
      {#each $virtualizer.getVirtualItems() as entry (entry.key)}
        <div style={`height: ${rowHeight}px;`}>
          {@render children(items[entry.index], entry.index)}
        </div>
      {/each}
    </div>
  </div>
</ScrollArea>
