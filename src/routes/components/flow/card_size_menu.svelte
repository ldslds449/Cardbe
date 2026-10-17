<script lang="ts">
  import * as ContextMenu from "$lib/components/ui/context-menu";
  import * as m from "$lib/paraglide/messages";
  import ScalingIcon from "@lucide/svelte/icons/scaling";
  import ZoomInIcon from "@lucide/svelte/icons/zoom-in";
  import ZoomOutIcon from "@lucide/svelte/icons/zoom-out";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import { formatNumber } from "$lib/i18n";
  import { clamp_scale } from "./flow";
  let {
    scale,
    onScale,
  }: {
    scale: number;
    onScale: (change: (scale: number) => number) => void;
  } = $props();
</script>

<ContextMenu.Sub>
  <ContextMenu.SubTrigger
    ><ScalingIcon />{m.flow_card_size()}<span
      class="ml-auto pl-4 text-xs text-muted-foreground tabular-nums"
      >{formatNumber(scale, { style: "percent" })}</span
    ></ContextMenu.SubTrigger
  >
  <ContextMenu.SubContent>
    <!-- Stepping keeps the menu open so the size can be changed repeatedly. -->
    <ContextMenu.Item
      closeOnSelect={false}
      disabled={scale >= 2}
      onSelect={() => onScale((value) => clamp_scale(value + 0.1))}
      ><ZoomInIcon />{m.flow_card_zoom_in()}</ContextMenu.Item
    >
    <ContextMenu.Item
      closeOnSelect={false}
      disabled={scale <= 0.5}
      onSelect={() => onScale((value) => clamp_scale(value - 0.1))}
      ><ZoomOutIcon />{m.flow_card_zoom_out()}</ContextMenu.Item
    >
    <ContextMenu.Separator />
    <ContextMenu.Item disabled={scale === 1} onSelect={() => onScale(() => 1)}
      ><RotateCcwIcon />{m.flow_card_size_reset()}</ContextMenu.Item
    >
  </ContextMenu.SubContent>
</ContextMenu.Sub>
