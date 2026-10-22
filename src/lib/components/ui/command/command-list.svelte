<script lang="ts">
  import { Command as CommandPrimitive } from "bits-ui";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { cn } from "$lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: CommandPrimitive.ListProps = $props();
</script>

<CommandPrimitive.List
  bind:ref
  data-slot="command-list"
  class={cn("flex max-h-[300px] flex-col overflow-hidden", className)}
  {...restProps}
>
  <!-- The list's max height caps the viewport, which scrolls items into view. -->
  <ScrollArea
    class="max-h-[inherit] min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[inherit] [&>[data-slot=scroll-area-viewport]]:scroll-py-1"
  >
    {@render children?.()}
  </ScrollArea>
</CommandPrimitive.List>
