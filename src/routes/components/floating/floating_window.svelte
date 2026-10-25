<script lang="ts">
  import { logger } from "#lib/logger.js";
  import { persist_floating_window_position } from "#lib/floating-window-state.js";
  import { Menu } from "@tauri-apps/api/menu";
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
          {
            text: open_label,
            enabled: can_open,
            action: () => after_menu(onOpen),
          },
          { item: "Separator" },
          {
            text: close_label,
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
    const position_listener = persist_floating_window_position(
      float_window,
    ).catch((error) => {
      logger.warn(`${log_scope}.position_listener.failed`, error);
    });
    return () => {
      void context_menu?.close();
      void position_listener.then((unlisten) => unlisten?.());
    };
  });
</script>

<svelte:head>
  <title>{title}</title>
</svelte:head>

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
