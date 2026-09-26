<script lang="ts">
import { useDroppable, type UseDroppableArguments } from "@dnd-kit-svelte/core";
import type { ClassValue } from "clsx";
import type { Snippet } from "svelte";

interface DroppableProps extends UseDroppableArguments {
  children: Snippet;
  class?: ClassValue;
}

let {
  children,
  class: className,
  id,
  disabled,
  data,
  resizeObserverConfig,
}: DroppableProps = $props();

// `useDroppable` reads these values reactively. Passing a rest object here
// captures the initial props in Svelte 5, so expose each prop through a
// getter instead.
const droppable = useDroppable({
  get id() {
    return id;
  },
  get disabled() {
    return disabled;
  },
  get data() {
    return data;
  },
  get resizeObserverConfig() {
    return resizeObserverConfig;
  },
});
</script>

<div class={[className]} bind:this={droppable.node.current}>
  {@render children()}
</div>
