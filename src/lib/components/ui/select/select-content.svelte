<script lang="ts">
  import { Select as SelectPrimitive } from "bits-ui";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { cn, type WithoutChild } from "#lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    sideOffset = 4,
    portalProps,
    children,
    ...restProps
  }: WithoutChild<SelectPrimitive.ContentProps> & {
    portalProps?: SelectPrimitive.PortalProps;
  } = $props();
</script>

<SelectPrimitive.Portal {...portalProps}>
  <SelectPrimitive.Content
    bind:ref
    {sideOffset}
    data-slot="select-content"
    class={cn(
      "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 max-h-(--bits-select-content-available-height) origin-(--bits-select-content-transform-origin) relative z-50 flex min-w-[8rem] flex-col overflow-hidden rounded-md border shadow-md data-[side=bottom]:translate-y-1 data-[side=left]:-translate-x-1 data-[side=right]:translate-x-1 data-[side=top]:-translate-y-1",
      className,
    )}
    {...restProps}
  >
    <!-- ScrollArea owns scrolling; the bits-ui viewport grows with its items
         (overriding its inline overflow) so the list is not clipped to the
         trigger height behind hover-to-scroll arrow buttons. -->
    <ScrollArea
      type="auto"
      class="max-h-[inherit] min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[inherit]"
    >
      <SelectPrimitive.Viewport
        class="min-w-(--bits-select-anchor-width) w-full scroll-my-1 p-1 overflow-visible!"
      >
        {@render children?.()}
      </SelectPrimitive.Viewport>
    </ScrollArea>
  </SelectPrimitive.Content>
</SelectPrimitive.Portal>
