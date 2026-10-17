<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Attachment } from "svelte/attachments";
  import { useSortable } from "@dnd-kit-svelte/svelte/sortable";

  let {
    id,
    index,
    disabled,
    class: class_name,
    children,
  }: {
    id: string;
    index: number;
    disabled: boolean;
    class: string;
    // The tab button is a <button>, so it must be the handle to start drags.
    children: Snippet<[Attachment<Element>]>;
  } = $props();

  const { ref, handleRef, isDragging } = useSortable({
    id: () => id,
    index: () => index,
    type: "flow-tab",
    accept: "flow-tab",
    group: "flow-tabs",
    disabled: () => disabled,
  });
</script>

<div class={class_name} class:opacity-50={isDragging.current} {@attach ref}>
  {@render children(handleRef)}
</div>
