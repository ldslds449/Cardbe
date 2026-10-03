<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { cn } from "$lib/utils.js";

  let {
    items,
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
  let scroll_top = $state(0);
  let viewport_height = $state(640);
  let previous_reset_key = "";
  let resize_frame: number | undefined;

  const start_index = $derived(
    Math.max(0, Math.floor(scroll_top / rowHeight) - overscan),
  );
  const end_index = $derived(
    Math.min(
      items.length,
      Math.ceil((scroll_top + viewport_height) / rowHeight) + overscan,
    ),
  );
  const visible_items = $derived(
    items.slice(start_index, end_index).map((item, offset) => ({
      item,
      index: start_index + offset,
    })),
  );

  function update_viewport() {
    if (!viewport) {
      return;
    }
    const next_scroll_top = viewport.scrollTop;
    const next_viewport_height = viewport.clientHeight || 640;
    if (scroll_top !== next_scroll_top) {
      scroll_top = next_scroll_top;
    }
    if (viewport_height !== next_viewport_height) {
      viewport_height = next_viewport_height;
    }
  }

  function handle_scroll() {
    update_viewport();
  }

  $effect(() => {
    const node = viewport;
    if (!node) {
      return;
    }
    node.addEventListener("scroll", handle_scroll, { passive: true });
    update_viewport();
    const observer = new ResizeObserver(() => {
      if (resize_frame !== undefined) {
        return;
      }
      resize_frame = requestAnimationFrame(() => {
        resize_frame = undefined;
        update_viewport();
      });
    });
    observer.observe(node);
    return () => {
      node.removeEventListener("scroll", handle_scroll);
      observer.disconnect();
      if (resize_frame !== undefined) {
        cancelAnimationFrame(resize_frame);
        resize_frame = undefined;
      }
    };
  });

  $effect(() => {
    if (resetKey === previous_reset_key) {
      return;
    }
    previous_reset_key = resetKey;
    if (viewport) {
      viewport.scrollTop = 0;
    }
    scroll_top = 0;
  });

  $effect(() => {
    if (
      hasMore &&
      !loading &&
      end_index >= Math.max(0, items.length - overscan * 2)
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
  <div class="relative" style={`height: ${items.length * rowHeight}px;`}>
    <div
      class="absolute left-0 right-5 top-0"
      style={`transform: translateY(${start_index * rowHeight}px);`}
    >
      {#each visible_items as entry (entry.index)}
        <div style={`height: ${rowHeight}px;`}>
          {@render children(entry.item, entry.index)}
        </div>
      {/each}
    </div>
  </div>
</ScrollArea>
