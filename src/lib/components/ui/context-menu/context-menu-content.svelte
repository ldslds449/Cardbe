<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { cn } from "#lib/utils.js";

  let {
    ref = $bindable(null),
    portalProps,
    class: className,
    children,
    ...restProps
  }: ContextMenuPrimitive.ContentProps & {
    portalProps?: ContextMenuPrimitive.PortalProps;
  } = $props();
</script>

<ContextMenuPrimitive.Portal {...portalProps}>
  <ContextMenuPrimitive.Content
    bind:ref
    data-slot="context-menu-content"
    class={cn(
      "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 max-h-(--bits-context-menu-content-available-height) origin-(--bits-context-menu-content-transform-origin) z-50 box-content flex min-w-[8rem] flex-col overflow-hidden rounded-md border shadow-md",
      className,
    )}
    {...restProps}
  >
    <!-- Content-box sizing lets the viewport inherit the space inside the border.
      The scrollbar only exists while items overflow; then items stop before it
      so their highlight is not covered. -->
    <ScrollArea
      type="auto"
      class="max-h-[inherit] min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[inherit] [&>[data-slot=scroll-area-viewport]]:p-1 [&:has(>[data-slot=scroll-area-scrollbar])>[data-slot=scroll-area-viewport]]:pe-2.5"
    >
      {@render children?.()}
    </ScrollArea>
  </ContextMenuPrimitive.Content>
</ContextMenuPrimitive.Portal>
