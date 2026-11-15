<script lang="ts">
  import type { HTMLTableAttributes } from "svelte/elements";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { cn, type WithElementRef } from "#lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLTableAttributes> = $props();
</script>

<ScrollArea data-slot="table-container" orientation="horizontal" class="w-full">
  <table
    bind:this={ref}
    data-slot="table"
    class={cn("w-full caption-bottom text-sm", className)}
    {...restProps}
  >
    {@render children?.()}
  </table>
</ScrollArea>
