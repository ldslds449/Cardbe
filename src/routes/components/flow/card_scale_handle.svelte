<script lang="ts">
  import { useSvelteFlow } from "@xyflow/svelte";
  import * as m from "$lib/paraglide/messages";
  import { clamp_scale } from "./flow";
  let {
    id,
    scale,
    target,
    base_width,
    onPreview,
    onCommit,
  }: {
    id: string;
    scale: number;
    target: HTMLElement | null;
    // Fixed-width nodes also need their width scaled while dragging.
    base_width?: number;
    onPreview: (scale: number | null) => void;
    onCommit: (scale: number) => void;
  } = $props();
  const { getNode, updateNode } = useSvelteFlow();
  // The opposite corner stays in place while a corner is dragged.
  const corners = [
    { x: -1, y: -1, class: "-top-1.5 -left-1.5 cursor-nwse-resize" },
    { x: 1, y: -1, class: "-top-1.5 -right-1.5 cursor-nesw-resize" },
    { x: -1, y: 1, class: "-bottom-1.5 -left-1.5 cursor-nesw-resize" },
    { x: 1, y: 1, class: "-right-1.5 -bottom-1.5 cursor-nwse-resize" },
  ] as const;
  let start: {
    corner: (typeof corners)[number];
    x: number;
    y: number;
    screen: { width: number; height: number };
    position: { x: number; y: number };
    size: { width: number; height: number };
    scale: number;
  } | null = null;
  let current = 1;
  function pointerdown(event: PointerEvent, corner: (typeof corners)[number]) {
    const node = getNode(id);
    if (event.button !== 0 || !target || !node?.measured?.width) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    // Screen sizes include the viewport zoom; measured sizes are in flow units.
    const rect = target.getBoundingClientRect();
    start = {
      corner,
      x: event.clientX,
      y: event.clientY,
      screen: { width: rect.width, height: rect.height },
      position: { ...node.position },
      size: {
        width: node.measured?.width ?? 0,
        height: node.measured?.height ?? 0,
      },
      scale,
    };
    current = scale;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }
  function pointermove(event: PointerEvent) {
    if (!start) {
      return;
    }
    const { corner, screen } = start;
    const factor =
      ((screen.width + corner.x * (event.clientX - start.x)) / screen.width +
        (screen.height + corner.y * (event.clientY - start.y)) /
          screen.height) /
      2;
    current = clamp_scale(start.scale * factor);
    const ratio = current / start.scale;
    const shrink = {
      x: start.size.width * (1 - ratio),
      y: start.size.height * (1 - ratio),
    };
    updateNode(id, {
      position: {
        x: start.position.x + (corner.x < 0 ? shrink.x : 0),
        y: start.position.y + (corner.y < 0 ? shrink.y : 0),
      },
      ...(base_width ? { width: base_width * current } : {}),
    });
    onPreview(current);
  }
  function finish() {
    if (!start) {
      return;
    }
    const changed = current !== start.scale;
    start = null;
    onPreview(null);
    if (changed) {
      onCommit(current);
    }
  }
</script>

<!-- Pointer-only; keyboard users pick a size from the card menu. -->
{#each corners as corner (corner.class)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class={`nodrag nopan absolute z-10 size-3 rounded-sm border-2 border-primary bg-background ${corner.class}`}
    title={m.flow_card_scale_handle()}
    aria-hidden="true"
    onpointerdown={(event) => pointerdown(event, corner)}
    onpointermove={pointermove}
    onpointerup={finish}
    onpointercancel={finish}
    onlostpointercapture={finish}
  ></div>
{/each}
