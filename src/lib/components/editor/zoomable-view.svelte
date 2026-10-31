<script lang="ts" module>
  const ZOOM_STEPS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 10];

  export function nextZoom(scale: number, direction: 1 | -1) {
    const steps = direction > 0 ? ZOOM_STEPS : [...ZOOM_STEPS].reverse();
    return (
      steps.find((step) => (direction > 0 ? step > scale : step < scale)) ??
      scale
    );
  }
</script>

<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import type { Snippet } from "svelte";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { cn } from "#lib/utils.js";
  import MaximizeIcon from "@lucide/svelte/icons/maximize-2";
  import ZoomInIcon from "@lucide/svelte/icons/zoom-in";
  import ZoomOutIcon from "@lucide/svelte/icons/zoom-out";

  // Adds zoom controls to an image or diagram and a full-window view. Zoom
  // resizes the content's layout box so ScrollArea scrolls the enlarged area.
  let {
    label,
    inline = false,
    class: className,
    contentClass,
    children,
  }: {
    label: string;
    inline?: boolean;
    class?: string;
    // Also applied in the full-screen view, which is portaled to the body.
    contentClass?: string;
    children: Snippet;
  } = $props();

  let scale = $state(1);
  // Rendered width at 100%, measured when zooming starts.
  let base_width = $state<number | null>(null);
  let content = $state<HTMLElement | null>(null);

  let open = $state(false);
  let full_scale = $state(1);
  let aspect = $state(1);
  let full_size = $state<readonly ResizeObserverSize[]>();
  const fit_width = $derived.by(() => {
    const size = full_size?.[0];
    if (!size) {
      return null;
    }
    return Math.max(
      1,
      Math.min(size.inlineSize, size.blockSize * (aspect || 1)),
    );
  });

  function content_rect() {
    return content?.firstElementChild?.getBoundingClientRect() ?? null;
  }

  function set_scale(value: number) {
    if (value === 1) {
      base_width = null;
    } else if (base_width === null) {
      base_width = content_rect()?.width || null;
    }
    scale = base_width === null ? 1 : value;
  }

  function open_full() {
    const rect = content_rect();
    aspect = rect && rect.height > 0 ? rect.width / rect.height : 1;
    full_scale = 1;
    open = true;
  }

  function full_keydown(event: KeyboardEvent) {
    if (event.ctrlKey || event.metaKey || event.altKey) {
      return;
    }
    if (event.key === "+" || event.key === "=") {
      full_scale = nextZoom(full_scale, 1);
    } else if (event.key === "-") {
      full_scale = nextZoom(full_scale, -1);
    } else if (event.key === "0") {
      full_scale = 1;
    } else {
      return;
    }
    event.preventDefault();
  }

  let full_viewport = $state<HTMLDivElement | null>(null);
  // Ctrl/Cmd + wheel zooms; Svelte wheel handlers are passive, so attach a
  // listener that can prevent the page zoom.
  $effect(() => {
    const viewport = full_viewport;
    if (!viewport) {
      return;
    }
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) {
        return;
      }
      event.preventDefault();
      full_scale = nextZoom(full_scale, event.deltaY < 0 ? 1 : -1);
    };
    viewport.addEventListener("wheel", wheel, { passive: false });
    return () => viewport.removeEventListener("wheel", wheel);
  });

  // Keep control clicks away from card selection, drag, and edit handlers.
  function control(action: () => void) {
    return (event: MouseEvent) => {
      event.stopPropagation();
      action();
    };
  }
</script>

{#snippet controls(
  value: number,
  set: (value: number) => void,
  extra?: Snippet,
)}
  <Button
    type="button"
    variant="ghost"
    size="icon-xs"
    aria-label={m.markdown_zoom_out()}
    title={m.markdown_zoom_out()}
    disabled={value <= ZOOM_STEPS[0]}
    ondblclick={(event) => event.stopPropagation()}
    onclick={control(() => set(nextZoom(value, -1)))}
  >
    <ZoomOutIcon />
  </Button>
  <Button
    type="button"
    variant="ghost"
    size="xs"
    class="min-w-12 tabular-nums"
    aria-label={m.markdown_zoom_reset()}
    title={m.markdown_zoom_reset()}
    ondblclick={(event) => event.stopPropagation()}
    onclick={control(() => set(1))}
  >
    {formatNumber(value, { style: "percent" })}
  </Button>
  <Button
    type="button"
    variant="ghost"
    size="icon-xs"
    aria-label={m.markdown_zoom_in()}
    title={m.markdown_zoom_in()}
    disabled={value >= ZOOM_STEPS.at(-1)!}
    ondblclick={(event) => event.stopPropagation()}
    onclick={control(() => set(nextZoom(value, 1)))}
  >
    <ZoomInIcon />
  </Button>
  {@render extra?.()}
{/snippet}

{#snippet full_button()}
  <Button
    type="button"
    variant="ghost"
    size="icon-xs"
    aria-label={m.markdown_view_full()}
    title={m.markdown_view_full()}
    ondblclick={(event) => event.stopPropagation()}
    onclick={control(open_full)}
  >
    <MaximizeIcon />
  </Button>
{/snippet}

<span
  data-no-drag
  class={cn(
    "zoomable-view group/zoom relative max-w-full",
    inline ? "inline-block align-bottom" : "block",
    className,
  )}
>
  <ScrollArea
    type="auto"
    orientation="horizontal"
    class={cn(
      "zoom-scroll min-w-0 max-w-full overflow-hidden",
      inline && "inline-block",
    )}
  >
    <span
      bind:this={content}
      class={cn(
        "zoom-content",
        base_width !== null && "zoom-content-scaled",
        contentClass,
      )}
      style:width={base_width !== null ? `${base_width * scale}px` : undefined}
    >
      {@render children()}
    </span>
  </ScrollArea>
  <span
    data-zoom-controls
    class="absolute top-1 right-1 z-10 flex items-center gap-0.5 rounded-md border bg-background/90 p-0.5 opacity-0 shadow-sm transition-opacity group-hover/zoom:opacity-100 focus-within:opacity-100"
  >
    {@render controls(scale, set_scale, full_button)}
  </span>
</span>

<Dialog.Root bind:open>
  <Dialog.Content
    class="flex h-[calc(100vh-2rem)] max-h-none w-[calc(100vw-2rem)] max-w-none flex-col gap-3 p-4 sm:max-w-none"
    onkeydown={full_keydown}
  >
    <Dialog.Header class="flex-row items-center gap-2 pe-8">
      <Dialog.Title class="min-w-0 flex-1 truncate">{label}</Dialog.Title>
      <div class="flex items-center gap-0.5">
        {@render controls(full_scale, (value) => (full_scale = value))}
      </div>
    </Dialog.Header>
    <!-- Bits UI sets an inline `position: relative` on the ScrollArea root,
         so size it as a flex item instead of positioning it absolutely. -->
    <div class="flex min-h-0 flex-1 flex-col" bind:contentBoxSize={full_size}>
      <ScrollArea
        type="auto"
        orientation="both"
        class="zoom-full-scroll min-h-0 flex-1 overflow-hidden"
        bind:viewport={full_viewport}
      >
        <!-- The measured min-height and auto margins center small content
             without pushing enlarged content past the scrollable start. -->
        <div
          class="flex min-w-full"
          style:min-height={full_size?.[0]
            ? `${full_size[0].blockSize}px`
            : undefined}
        >
          <span
            class={cn("zoom-content zoom-content-scaled m-auto", contentClass)}
            style:width={fit_width !== null
              ? `${fit_width * full_scale}px`
              : undefined}
          >
            {@render children()}
          </span>
        </div>
      </ScrollArea>
    </div>
  </Dialog.Content>
</Dialog.Root>
