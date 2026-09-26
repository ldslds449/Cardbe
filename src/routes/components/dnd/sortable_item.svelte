<script lang="ts">
import type { UniqueIdentifier } from "@dnd-kit-svelte/core";
import { CSS, styleObjectToString } from "@dnd-kit-svelte/utilities";
import { useSortable } from "@dnd-kit-svelte/sortable";
import { untrack } from "svelte";
import type { HTMLAttributes } from "svelte/elements";

interface Identifier {
  sort_id: UniqueIdentifier;
}

let {
  sort_id,
  children,
  ...restProps
}: HTMLAttributes<HTMLDivElement> & Identifier = $props();

const {
  attributes,
  listeners,
  node,
  transform,
  transition,
  isDragging,
  isSorting,
} = useSortable({
  id: untrack(() => sort_id),
});

const style = $derived(
  (function () {
    let new_transform = null;
    if (transform.current) {
      new_transform = {
        x: transform.current.x,
        y: transform.current.y,
        scaleX: 1,
        scaleY: 1,
      };
    }
    return styleObjectToString({
      transform: CSS.Transform.toString(new_transform),
      transition: isSorting.current ? transition.current : undefined,
      zIndex: isDragging.current ? 1 : undefined,
      opacity: isDragging.current ? "65%" : undefined,
    });
  })(),
);
</script>

<div
  class="relative select-none"
  bind:this={node.current}
  {style}
  {...listeners.current}
  {...attributes.current}
  {...restProps}
>
  {@render children?.()}
</div>
