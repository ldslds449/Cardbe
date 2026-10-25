<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { cn } from "#lib/utils.js";
  import type { WidthBounds } from "../layout-widths.svelte";

  const KEYBOARD_STEP = 16;
  const VIEWPORT_MARGIN = 32;

  let {
    width,
    bounds,
    label,
    edge = "end",
    // Centered dialogs grow on both sides, so pointer movement counts twice.
    scale = 1,
    // Keep dialogs inside the viewport while dragging.
    fit_viewport = false,
    class: className,
    onresize,
  }: {
    width: number;
    bounds: WidthBounds;
    label: string;
    edge?: "start" | "end";
    scale?: number;
    fit_viewport?: boolean;
    class?: string;
    onresize: (width: number, commit: boolean) => void;
  } = $props();

  let dragging = $state(false);

  function max_width(): number {
    return fit_viewport
      ? Math.max(
          bounds.min,
          Math.min(bounds.max, window.innerWidth - VIEWPORT_MARGIN),
        )
      : bounds.max;
  }

  function clamp(value: number): number {
    return Math.round(Math.min(max_width(), Math.max(bounds.min, value)));
  }

  // Native listeners stop pointer and key events before they reach the column's sortable
  // drag sensor and its double-click task creation, which Svelte's delegated
  // handlers cannot do.
  function attach_pointer(node: HTMLElement) {
    let start_x = 0;
    let start_width = 0;
    let latest = 0;
    let previous_cursor = "";
    let previous_select = "";

    const direction = () => (edge === "end" ? 1 : -1);

    function on_move(event: PointerEvent) {
      latest = clamp(
        start_width + (event.clientX - start_x) * direction() * scale,
      );
      onresize(latest, false);
    }

    function finish(event: PointerEvent) {
      if (!dragging) {
        return;
      }
      dragging = false;
      if (node.hasPointerCapture(event.pointerId)) {
        node.releasePointerCapture(event.pointerId);
      }
      document.body.style.cursor = previous_cursor;
      document.body.style.userSelect = previous_select;
      node.removeEventListener("pointermove", on_move);
      onresize(latest, true);
    }

    function on_down(event: PointerEvent) {
      event.stopPropagation();
      if (event.button !== 0) {
        return;
      }
      event.preventDefault();
      start_x = event.clientX;
      start_width = Math.min(width, max_width());
      latest = start_width;
      dragging = true;
      node.setPointerCapture(event.pointerId);
      previous_cursor = document.body.style.cursor;
      previous_select = document.body.style.userSelect;
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      node.addEventListener("pointermove", on_move);
    }

    function on_double_click(event: MouseEvent) {
      event.stopPropagation();
      event.preventDefault();
      onresize(clamp(bounds.default), true);
    }

    const stop = (event: Event) => event.stopPropagation();

    node.addEventListener("keydown", handle_keydown);
    node.addEventListener("pointerdown", on_down);
    node.addEventListener("pointerup", finish);
    node.addEventListener("pointercancel", finish);
    node.addEventListener("dblclick", on_double_click);
    node.addEventListener("click", stop);
    node.addEventListener("contextmenu", stop);
    return () => {
      node.removeEventListener("keydown", handle_keydown);
      node.removeEventListener("pointerdown", on_down);
      node.removeEventListener("pointerup", finish);
      node.removeEventListener("pointercancel", finish);
      node.removeEventListener("pointermove", on_move);
      node.removeEventListener("dblclick", on_double_click);
      node.removeEventListener("click", stop);
      node.removeEventListener("contextmenu", stop);
    };
  }

  function handle_keydown(event: KeyboardEvent) {
    let next: number | null = null;
    const step = event.shiftKey ? KEYBOARD_STEP * 4 : KEYBOARD_STEP;
    const grow = edge === "end" ? "ArrowRight" : "ArrowLeft";
    const shrink = edge === "end" ? "ArrowLeft" : "ArrowRight";
    if (event.key === grow) {
      next = width + step;
    } else if (event.key === shrink) {
      next = width - step;
    } else if (event.key === "Home") {
      next = bounds.min;
    } else if (event.key === "End") {
      next = max_width();
    } else if (event.key === "Enter") {
      next = bounds.default;
    }
    if (next === null) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    onresize(clamp(next), true);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={width}
  aria-valuemin={bounds.min}
  aria-valuemax={bounds.max}
  title={m.layout_resize_hint()}
  tabindex="0"
  class={cn(
    "group/resize absolute inset-y-0 z-10 flex w-3 cursor-col-resize touch-none justify-center outline-none",
    className,
  )}
  data-dragging={dragging ? "" : undefined}
  {@attach attach_pointer}
>
  <div
    class="h-full w-0.5 rounded-full bg-transparent transition-colors group-hover/resize:bg-border group-focus-visible/resize:bg-ring group-data-[dragging]/resize:bg-ring"
  ></div>
</div>
