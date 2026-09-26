<script lang="ts">
import { useSortable } from "@dnd-kit-svelte/svelte/sortable";
import GripVerticalIcon from "@lucide/svelte/icons/grip-vertical";
import type { Snippet } from "svelte";

let {
  item_id,
  index,
  children,
}: {
  item_id: string;
  index: number;
  children: Snippet;
} = $props();

const { ref, handleRef, isDragging } = useSortable({
  id: () => item_id,
  index: () => index,
  type: "checklist-item",
  accept: "checklist-item",
  group: "checklist",
});
</script>

<div
  role="listitem"
  class="flex items-center gap-1.5 rounded-md border bg-background p-1.5 transition-[opacity,box-shadow]"
  class:opacity-50={isDragging.current}
  class:shadow-md={isDragging.current}
  {@attach ref}
>
  <button
    type="button"
    class="flex size-7 shrink-0 touch-none cursor-grab items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
    aria-label="Reorder checklist item"
    title="Drag to reorder"
    {@attach handleRef}
  >
    <GripVerticalIcon class="size-4" />
  </button>
  {@render children()}
</div>
