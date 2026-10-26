<script lang="ts">
  import { useSortable } from "@dnd-kit-svelte/svelte/sortable";
  import GripVerticalIcon from "@lucide/svelte/icons/grip-vertical";
  import { Button } from "#lib/components/ui/button/index.js";
  import { m } from "#lib/paraglide/messages.js";
  import type { Snippet } from "svelte";

  let {
    id,
    index,
    disabled,
    children,
  }: {
    id: string;
    index: number;
    disabled: boolean;
    children: Snippet;
  } = $props();
  const { ref, handleRef, isDragging } = useSortable({
    id: () => id,
    index: () => index,
    type: "daily-plan-task",
    accept: "daily-plan-task",
    group: "daily-plan",
    disabled: () => disabled,
  });
</script>

<li
  {@attach ref}
  class="group flex items-center gap-2 rounded-md px-2 py-2.5 transition-colors hover:bg-muted/40"
  class:opacity-50={isDragging.current}
>
  <Button
    {@attach handleRef}
    variant="ghost"
    size="icon-xs"
    {disabled}
    class="shrink-0 touch-none cursor-grab text-muted-foreground active:cursor-grabbing"
    aria-label={m.ui_drag_to_reorder()}
    title={m.ui_drag_to_reorder()}><GripVerticalIcon class="size-4" /></Button
  >
  {@render children()}
</li>
