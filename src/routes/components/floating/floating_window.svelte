<script lang="ts">
  import { logger } from "#lib/logger.js";
  import { invoke } from "@tauri-apps/api/core";
  import { toast } from "svelte-sonner";
  import { translateCommandError } from "#lib/command-errors.js";
  import * as m from "#lib/paraglide/messages.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import {
    persist_floating_window_position,
    save_window_state,
  } from "#lib/floating-window-state.js";
  import { compact_widget } from "#lib/desktop-widget.svelte.js";
  import { Menu } from "@tauri-apps/api/menu";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, type Snippet } from "svelte";

  // The shell shared by floating task and Focus windows: the window title,
  // dragging, double-click to open in Cardbe, and the native context menu.
  let {
    title,
    busy = false,
    hint,
    open_label,
    can_open,
    close_label,
    log_scope,
    onOpen,
    onClose,
    children,
  }: {
    /** The window title, shown by the OS. */
    title: string;
    busy?: boolean;
    /** Explains dragging and double-clicking. */
    hint: string;
    open_label: string;
    can_open: boolean;
    close_label: string;
    /** Prefix for log events, such as `floating_task`. */
    log_scope: string;
    onOpen: () => void;
    onClose: () => void;
    children: Snippet;
  } = $props();

  const float_window = getCurrentWindow();
  const desktop = float_window.label.startsWith("widget-");
  let compact = $state(false);

  async function read_widget_size() {
    const [size, scale] = await Promise.all([
      float_window.innerSize(),
      float_window.scaleFactor(),
    ]);
    const logical = size.toLogical(scale);
    compact = compact_widget(logical.width, logical.height);
  }

  // Desktop widgets resize from a corner grip. Native edge resizing is not
  // reliable once Windows reparents the window under the desktop, so the
  // size follows the pointer directly. Matches `min_inner_size` in Rust.
  const MIN_WIDGET_WIDTH = 160;
  const MIN_WIDGET_HEIGHT = 120;
  let resize_start: {
    x: number;
    y: number;
    width: number;
    height: number;
  } | null = null;
  let resize_frame = 0;

  function start_resize(event: PointerEvent) {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
    resize_start = {
      x: event.screenX,
      y: event.screenY,
      width: window.innerWidth,
      height: window.innerHeight,
    };
  }

  function resize_to_pointer(event: PointerEvent) {
    if (!resize_start) {
      return;
    }
    const width = Math.max(
      MIN_WIDGET_WIDTH,
      Math.round(resize_start.width + event.screenX - resize_start.x),
    );
    const height = Math.max(
      MIN_WIDGET_HEIGHT,
      Math.round(resize_start.height + event.screenY - resize_start.y),
    );
    cancelAnimationFrame(resize_frame);
    resize_frame = requestAnimationFrame(() => {
      void float_window
        .setSize(new LogicalSize(width, height))
        .catch((caught) =>
          logger.warn(`${log_scope}.widget_size.failed`, caught),
        );
    });
  }

  function end_resize() {
    resize_start = null;
  }

  $effect(() => {
    void float_window
      .setTitle(title)
      .catch((caught) => logger.warn(`${log_scope}.title.failed`, caught));
  });

  // The whole window is a drag handle apart from its controls. Like Tauri's
  // own drag region, this uses mousedown, whose detail counts clicks (pointer
  // events report 0), and a second press is a double-click: starting another
  // drag would swallow it.
  function handle_mousedown(event: MouseEvent) {
    if (
      event.button !== 0 ||
      (event.target as Element | null)?.closest(
        "a, button, input, [data-slot='scroll-area-scrollbar']",
      )
    ) {
      return;
    }
    if (event.detail === 2) {
      if (can_open) {
        onOpen();
      }
      return;
    }
    void float_window
      .startDragging()
      .catch((caught) => logger.warn(`${log_scope}.drag.failed`, caught));
  }

  // A native menu can extend past the small window instead of being clipped
  // by it. It is rebuilt per popup so labels and enabled state stay current.
  //
  // Tauri's popup command holds this webview's resource table lock while the
  // menu's modal loop runs on the main thread, and creating or closing a menu
  // locks that table on the main thread. Any such call during a popup, like a
  // second right-click, deadlocks the app, so nothing touches menus or runs
  // an item's action until the popup has returned.
  let context_menu: Menu | null = null;
  let menu_open = false;
  let pending_action: (() => void) | null = null;

  function after_menu(action: () => void) {
    if (menu_open) {
      pending_action = action;
    } else {
      action();
    }
  }

  async function show_context_menu(event: MouseEvent) {
    event.preventDefault();
    if (menu_open) {
      return;
    }
    menu_open = true;
    try {
      await context_menu?.close();
      context_menu = null;
      const menu = await Menu.new({
        items: [
          ...(desktop
            ? [
                {
                  text: m.desktop_widget_manage(),
                  action: () =>
                    after_menu(() => {
                      void invoke("open_desktop_widget_manager").catch(
                        (error) => toast.error(translateCommandError(error)),
                      );
                    }),
                },
              ]
            : []),
          {
            text: open_label,
            enabled: can_open,
            action: () => after_menu(onOpen),
          },
          { item: "Separator" },
          {
            text: desktop ? m.desktop_widget_remove() : close_label,
            action: () => after_menu(onClose),
          },
        ],
      });
      context_menu = menu;
      await menu.popup(undefined, float_window);
    } catch (caught) {
      logger.warn(`${log_scope}.menu.failed`, caught);
    } finally {
      menu_open = false;
      const action = pending_action;
      pending_action = null;
      action?.();
    }
  }

  onMount(() => {
    // Resizing is saved like moving, and updates the compact layout.
    let resize_timer: ReturnType<typeof setTimeout> | undefined;
    const sync_widget_size = () =>
      void read_widget_size().catch((caught) =>
        logger.warn(`${log_scope}.widget_size.failed`, caught),
      );
    const resize_listener = desktop
      ? float_window
          .onResized(() => {
            sync_widget_size();
            clearTimeout(resize_timer);
            cancelAnimationFrame(resize_frame);
            resize_timer = setTimeout(() => {
              void save_window_state().catch((caught) =>
                logger.warn(`${log_scope}.widget_size.failed`, caught),
              );
            }, 250);
          })
          .catch((caught) => {
            logger.warn(`${log_scope}.widget_size.failed`, caught);
          })
      : Promise.resolve(undefined);
    if (desktop) {
      sync_widget_size();
    }
    const position_listener = persist_floating_window_position(
      float_window,
    ).catch((error) => {
      logger.warn(`${log_scope}.position_listener.failed`, error);
    });
    return () => {
      void context_menu?.close();
      void position_listener.then((unlisten) => unlisten?.());
      clearTimeout(resize_timer);
      cancelAnimationFrame(resize_frame);
      void resize_listener.then((unlisten) => unlisten?.());
    };
  });
</script>

<svelte:head>
  <title>{title}</title>
</svelte:head>

{#if desktop}
  <main class="desktop-widget h-screen overflow-hidden" aria-busy={busy}>
    <!-- Fills the window, which Rust clips to this radius (`CORNER_RADIUS`):
      desktop children have no transparent margins for shadows. -->
    <Card.Root
      class="group/widget relative h-screen cursor-grab gap-0 select-none active:cursor-grabbing overflow-hidden rounded-2xl border-border/40 bg-card py-0 shadow-none"
      data-widget-size={compact ? "small" : undefined}
      oncontextmenu={show_context_menu}
      onmousedown={handle_mousedown}
    >
      <Button
        variant="ghost"
        size="icon-xs"
        class="absolute top-3 right-3 z-10 opacity-0 transition-opacity group-hover/widget:opacity-100 group-focus-within/widget:opacity-100 focus-visible:opacity-100"
        aria-label={m.desktop_widget_options()}
        title={m.desktop_widget_options()}
        onclick={show_context_menu}
      >
        <EllipsisIcon />
      </Button>
      {@render children()}
      <div
        aria-hidden="true"
        class="absolute right-1 bottom-1 z-10 size-4 cursor-nwse-resize touch-none p-1 opacity-0 transition-opacity group-hover/widget:opacity-100"
        onpointerdown={start_resize}
        onpointermove={resize_to_pointer}
        onpointerup={end_resize}
        onpointercancel={end_resize}
      >
        <span
          class="block size-full rounded-br-sm border-r-2 border-b-2 border-muted-foreground/60"
        ></span>
      </div>
    </Card.Root>
  </main>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <main
    class="flex h-screen cursor-grab select-none flex-col overflow-hidden border bg-background text-foreground outline-none active:cursor-grabbing"
    aria-busy={busy}
    title={hint}
    oncontextmenu={show_context_menu}
    onmousedown={handle_mousedown}
  >
    {@render children()}
  </main>
{/if}

<style>
  :global(body:has(.desktop-widget)) {
    background: transparent;
  }
</style>
