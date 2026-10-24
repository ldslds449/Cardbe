<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "$lib/command-errors";
  import {
    clamp_floating_zoom,
    forget_floating_task,
    read_floating_height,
    read_floating_zoom,
    write_floating_height,
    write_floating_zoom,
  } from "$lib/floating-tasks";
  import { formatNumber } from "$lib/i18n";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { Menu } from "@tauri-apps/api/menu";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";

  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import {
    deserialize_column,
    type ColumnSerialized,
  } from "../type/column.svelte";
  import { recurrence_label, type Task } from "../type/task.svelte";
  import { display_task_color } from "../utils/task-color";
  import LabelBadge from "../components/label_badge.svelte";
  import Markdown from "../components/markdown.svelte";
  import ChecklistDisplay from "../components/task/checklist_display.svelte";
  import { due_label, due_label_class } from "../components/task/due-label";

  interface BoardsState {
    boards: Array<{ id: number; name: string }>;
  }

  // Board data has no change notifications, so a light periodic read keeps
  // the reminder current alongside focus and quick-add change events.
  const REFRESH_INTERVAL_MS = 5000;

  const params = new URLSearchParams(window.location.search);
  const ref = {
    boardId: Number(params.get("board")),
    taskId: Number(params.get("task")),
  };
  const float_window = getCurrentWindow();

  let task = $state<Task | null>(null);
  let board_name = $state("");
  let column_name = $state("");
  let loading = $state(true);
  let missing = $state(false);
  let error = $state<CommandError | null>(null);
  let now = $state(new Date());
  let load_generation = 0;

  // Content scales through the root font size: text, spacing and icons all
  // use rem, so the layout stays intact inside the fixed-size window.
  let zoom = $state(read_floating_zoom(ref));
  let zoom_indicator_visible = $state(false);
  let zoom_indicator_timer: number | undefined;

  $effect(() => {
    document.documentElement.style.fontSize = zoom === 1 ? "" : `${zoom}rem`;
  });

  function set_zoom(next: number) {
    const clamped = clamp_floating_zoom(next);
    zoom_indicator_visible = true;
    window.clearTimeout(zoom_indicator_timer);
    zoom_indicator_timer = window.setTimeout(
      () => (zoom_indicator_visible = false),
      900,
    );
    if (clamped !== zoom) {
      zoom = clamped;
      write_floating_zoom(ref, clamped);
    }
  }

  const due = $derived(
    task?.due_time ? due_label(task.due_time, now, task.due_at_midnight) : null,
  );

  // The window height follows its content, so there is no empty bottom, up
  // to a share of the screen beyond which the body scrolls. Dragging can
  // shorten it, and the body then scrolls; the size constraints cap the
  // height at the content so a drag never adds empty space.
  const has_details = $derived(
    !!task &&
      (!!due ||
        !!task.recurrence ||
        task.labels.length > 0 ||
        task.items.length > 0 ||
        task.description.trim() !== ""),
  );
  const title_only = $derived(!!task && !has_details && !error);

  // Matches the minimum inner size in `open_floating_task_window`;
  // `setSizeConstraints` replaces every constraint at once.
  const MIN_WIDTH = 220;
  const MIN_HEIGHT = 32;
  const MAX_SCREEN_SHARE = 0.6;

  let header = $state<HTMLElement | null>(null);
  let body = $state<HTMLElement | null>(null);
  let fitted_height = 0;
  let content_height = 0;
  // A height the user dragged below the fit, kept while the content changes
  // and across launches.
  let chosen_height = read_floating_height(ref);
  let pending_resizes = 0;
  // Content that fits only overflows until the window catches up, as while
  // zooming in; the scrollbar is shown only when it really scrolls.
  let overflowing = $state(false);
  let resizing = Promise.resolve();

  function handle_window_resize() {
    // Resizes made here are not the user's choice.
    if (pending_resizes > 0 || fitted_height === 0) {
      return;
    }
    // One pixel absorbs logical-to-physical rounding.
    const shortened = window.innerHeight < fitted_height - 1;
    const next = shortened ? window.innerHeight : null;
    if (next !== chosen_height) {
      chosen_height = next;
      write_floating_height(ref, next);
    }
    overflowing = shortened || content_height > fitted_height;
  }

  function fit_window() {
    if (!header) {
      return;
    }
    // The main element's 1px borders add 2px. One more pixel absorbs the
    // logical-to-physical rounding under display scaling, which would
    // otherwise leave the body a fraction short and show a scrollbar.
    const content = Math.ceil(
      header.getBoundingClientRect().height +
        (body?.getBoundingClientRect().height ?? 0) +
        3,
    );
    const ceiling = Math.max(
      MIN_HEIGHT,
      Math.floor(window.screen.availHeight * MAX_SCREEN_SHARE),
    );
    const max_height = Math.min(Math.max(content, MIN_HEIGHT), ceiling);
    const height =
      chosen_height === null
        ? max_height
        : Math.min(Math.max(chosen_height, MIN_HEIGHT), max_height);
    content_height = content;
    overflowing = content > height;
    if (max_height === fitted_height && window.innerHeight === height) {
      return;
    }
    fitted_height = max_height;
    pending_resizes += 1;
    resizing = resizing
      .then(async () => {
        await float_window.setSizeConstraints({
          minWidth: MIN_WIDTH,
          minHeight: MIN_HEIGHT,
          maxHeight: max_height,
        });
        await float_window.setSize(new LogicalSize(window.innerWidth, height));
      })
      .catch((caught) => logger.warn("floating_task.resize.failed", caught))
      // The resize event can follow the call's return.
      .then(
        () =>
          new Promise<void>((resolve) =>
            requestAnimationFrame(() => resolve()),
          ),
      )
      .finally(() => (pending_resizes -= 1));
  }

  $effect(() => {
    if (loading || !header) {
      return;
    }
    const observer = new ResizeObserver(() => fit_window());
    observer.observe(header);
    if (body) {
      observer.observe(body);
    }
    fit_window();
    return () => observer.disconnect();
  });

  const window_title = $derived(
    task
      ? `${task.title} · Cardbe`
      : missing
        ? `${m.task_missing()} · Cardbe`
        : "Cardbe",
  );

  $effect(() => {
    void float_window
      .setTitle(window_title)
      .catch((caught) => logger.warn("floating_task.title.failed", caught));
  });

  async function load() {
    const generation = ++load_generation;
    try {
      const [state, columns] = await Promise.all([
        invoke<BoardsState>("get_boards"),
        invoke<ColumnSerialized[]>("get_board_columns", {
          boardId: ref.boardId,
        }),
      ]);
      if (generation !== load_generation) {
        return;
      }
      const task_key = `task_${ref.taskId}`;
      let found: { task: Task; column: string } | null = null;
      for (const column of columns.map(deserialize_column)) {
        const match = column.tasks.find(
          (candidate) => candidate.id === task_key,
        );
        if (match) {
          found = { task: match, column: column.name };
          break;
        }
      }
      board_name =
        state.boards.find((board) => board.id === ref.boardId)?.name ?? "";
      task = found?.task ?? null;
      column_name = found?.column ?? "";
      missing = found === null;
      error = null;
    } catch (caught) {
      if (generation !== load_generation) {
        return;
      }
      const parsed = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
      if (parsed.code === "BOARD_NOT_FOUND") {
        task = null;
        missing = true;
        error = null;
      } else {
        logger.warn("floating_task.load.failed", caught);
        error = parsed;
      }
    } finally {
      if (generation === load_generation) {
        if (loading) {
          announce_ready();
        }
        loading = false;
        now = new Date();
      }
    }
  }

  // Keeps the compact startup loader until the first read settles, instead
  // of flashing an empty window.
  function announce_ready() {
    requestAnimationFrame(() =>
      window.dispatchEvent(new Event("cardbe:workspace-ready")),
    );
  }

  onMount(() => {
    if (
      !Number.isSafeInteger(ref.boardId) ||
      !Number.isSafeInteger(ref.taskId)
    ) {
      loading = false;
      missing = true;
      announce_ready();
      return;
    }
    void load();
    const timer = window.setInterval(() => void load(), REFRESH_INTERVAL_MS);
    const handle_focus = () => void load();
    window.addEventListener("focus", handle_focus);
    const data_changed = listen("cardbe:data-changed", () => void load());
    // Non-passive so Ctrl + wheel can zoom instead of scrolling.
    const handle_wheel = (event: WheelEvent) => {
      if (!event.ctrlKey || event.deltaY === 0) {
        return;
      }
      event.preventDefault();
      set_zoom(zoom + (event.deltaY < 0 ? 0.1 : -0.1));
    };
    window.addEventListener("wheel", handle_wheel, { passive: false });
    return () => {
      void context_menu?.close();
      window.removeEventListener("wheel", handle_wheel);
      window.clearTimeout(zoom_indicator_timer);
      window.clearInterval(timer);
      window.removeEventListener("focus", handle_focus);
      void data_changed.then((unlisten) => unlisten());
    };
  });

  // The whole window is a drag handle. Like Tauri's own drag region, this uses
  // mousedown, whose detail counts clicks (pointer events report 0), and a
  // second press is a double-click: starting another drag would swallow it.
  function handle_mousedown(event: MouseEvent) {
    if (
      event.button !== 0 ||
      (event.target as Element | null)?.closest(
        "a, [data-slot='scroll-area-scrollbar']",
      )
    ) {
      return;
    }
    if (event.detail === 2) {
      if (task) {
        void open_in_cardbe();
      }
      return;
    }
    void float_window
      .startDragging()
      .catch((caught) => logger.warn("floating_task.drag.failed", caught));
  }

  async function open_in_cardbe() {
    try {
      await invoke("open_task_in_main_window", { ...ref });
    } catch (caught) {
      logger.warn("floating_task.open_main.failed", caught);
      const parsed = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
      if (
        parsed.code === "TASK_NOT_FOUND" ||
        parsed.code === "BOARD_NOT_FOUND"
      ) {
        task = null;
        missing = true;
      } else {
        error = parsed;
      }
    }
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
            text: m.floating_task_open_in_cardbe(),
            enabled: !!task,
            action: () => after_menu(() => void open_in_cardbe()),
          },
          { item: "Separator" },
          {
            text: m.floating_task_close(),
            action: () => after_menu(() => void close_floating_window()),
          },
        ],
      });
      context_menu = menu;
      await menu.popup(undefined, float_window);
    } catch (caught) {
      logger.warn("floating_task.menu.failed", caught);
    } finally {
      menu_open = false;
      const action = pending_action;
      pending_action = null;
      action?.();
    }
  }

  async function close_floating_window() {
    forget_floating_task(ref);
    try {
      await float_window.close();
    } catch (caught) {
      logger.warn("floating_task.close.failed", caught);
    }
  }
</script>

<svelte:window
  onresize={handle_window_resize}
  onkeydown={(event) => {
    if (event.key === "Enter" && task && event.target === document.body) {
      event.preventDefault();
      void open_in_cardbe();
    } else if (event.ctrlKey && !event.altKey && !event.metaKey) {
      const step =
        event.key === "=" || event.key === "+"
          ? 0.1
          : event.key === "-"
            ? -0.1
            : null;
      if (step !== null || event.key === "0") {
        event.preventDefault();
        set_zoom(step === null ? 1 : zoom + step);
      }
    }
  }}
/>

<svelte:head>
  <title>{window_title}</title>
</svelte:head>

<main
  class="flex h-screen select-none flex-col overflow-hidden border bg-background text-foreground outline-none"
  aria-busy={loading}
  oncontextmenu={show_context_menu}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="flex min-h-0 flex-1 cursor-grab flex-col active:cursor-grabbing"
    title={m.floating_task_hint()}
    onmousedown={handle_mousedown}
  >
    <header class="shrink-0 px-3 pt-2.5 pb-1.5" bind:this={header}>
      <h1
        class="line-clamp-2 text-sm leading-5 font-semibold [overflow-wrap:anywhere]"
        style:color={task ? display_task_color(task.color) : undefined}
      >
        {task?.title ?? (loading ? "" : m.task_missing())}
      </h1>
      {#if task}
        <p class="truncate text-xs text-muted-foreground">
          {board_name} · {column_name}
        </p>
      {/if}
    </header>

    {#if !title_only}
      <ScrollArea
        orientation="vertical"
        class="min-h-0 flex-1"
        scrollbarYClasses={overflowing ? "" : "hidden"}
      >
        <div class="space-y-2 px-3 pb-3" bind:this={body}>
          {#if error}
            <p class="text-xs text-destructive" aria-live="polite">
              {translateCommandError(error)}
            </p>
          {/if}
          {#if missing}
            <p class="text-xs text-muted-foreground">
              {m.floating_task_missing_description()}
            </p>
          {:else if task}
            {#if due || task.recurrence}
              <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                {#if due}
                  <span
                    class="inline-flex items-center gap-1.5 {due_label_class(
                      due.status,
                    )}"
                  >
                    {#if due.status === "overdue"}
                      <CircleAlertIcon class="size-3.5 shrink-0" />
                    {:else}
                      <ClockIcon class="size-3.5 shrink-0" />
                    {/if}
                    <span>{due.text}</span>
                  </span>
                {/if}
                {#if task.recurrence}
                  <span
                    class="inline-flex items-center gap-1.5 text-muted-foreground"
                  >
                    <Repeat2Icon class="size-3.5 shrink-0" />
                    <span>{recurrence_label(task.recurrence)}</span>
                  </span>
                {/if}
              </div>
            {/if}
            {#if task.labels.length > 0}
              <div class="flex flex-wrap gap-1">
                {#each task.labels as label (label)}
                  <LabelBadge {label} />
                {/each}
              </div>
            {/if}
            <ChecklistDisplay items={task.items} />
            {#if task.description.trim()}
              <div class="text-sm text-muted-foreground">
                <Markdown md={task.description} compact />
              </div>
            {/if}
          {/if}
        </div>
      </ScrollArea>
    {/if}
  </div>
  {#if zoom_indicator_visible}
    <div
      class="pointer-events-none fixed right-2 bottom-2 rounded-md border bg-popover px-2 py-0.5 text-xs text-popover-foreground shadow-sm"
      role="status"
    >
      {formatNumber(zoom, { style: "percent" })}
    </div>
  {/if}
</main>
