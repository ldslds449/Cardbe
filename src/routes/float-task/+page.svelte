<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "#lib/command-errors.js";
  import {
    clamp_floating_zoom,
    forget_floating_task,
    read_floating_collapsed,
    read_floating_height,
    read_floating_zoom,
    write_floating_height,
    write_floating_collapsed,
    write_floating_zoom,
  } from "#lib/floating-tasks.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { logger } from "#lib/logger.js";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, tick } from "svelte";

  import { Button } from "#lib/components/ui/button/index.js";
  import MinusIcon from "@lucide/svelte/icons/minus";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";

  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import {
    deserialize_column,
    type ColumnSerialized,
  } from "../type/column.svelte";
  import { recurrence_label, type Task } from "../type/task.svelte";
  import { display_task_color } from "../utils/task-color";
  import LabelBadge from "../components/label_badge.svelte";
  import PriorityIndicator from "../components/task/priority_indicator.svelte";
  import Markdown from "../components/markdown.svelte";
  import ChecklistDisplay from "../components/task/checklist_display.svelte";
  import DueBadge from "../components/task/due_badge.svelte";
  import FloatingWindow from "../components/floating/floating_window.svelte";

  interface BoardsState {
    boards: Array<{ id: number; name: string }>;
  }

  // Writes announce themselves with `cardbe:data-changed`; only the relative
  // due label needs a clock.
  const CLOCK_INTERVAL_MS = 60_000;
  // One edit can write several times; reload once after the burst.
  const RELOAD_DELAY_MS = 100;

  const params = new URLSearchParams(window.location.search);
  const ref = {
    boardId: Number(params.get("board")),
    taskId: Number(params.get("task")),
  };
  const float_window = getCurrentWindow();

  let collapsed = $state(read_floating_collapsed(ref));

  function toggle_collapsed() {
    collapsed = !collapsed;
    write_floating_collapsed(ref, collapsed);
  }
  let task = $state<Task | null>(null);
  let board_name = $state("");
  let column_name = $state("");
  let loading = $state(true);
  let missing = $state(false);
  let error = $state<CommandError | null>(null);
  let now = $state(new Date());
  let load_generation = 0;
  // Unchanged reads keep the rendered content, so writes elsewhere do not
  // re-render the description.
  let loaded_snapshot = "";

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

  // The window height follows its content, so there is no empty bottom, up
  // to a share of the screen beyond which the body scrolls. Dragging can
  // shorten it, and the body then scrolls; the size constraints cap the
  // height at the content so a drag never adds empty space.
  const has_details = $derived(
    !!task &&
      (!!task.due_time ||
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
  let fitted_collapsed = false;
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
    if (collapsed || pending_resizes > 0 || fitted_height === 0) {
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
        (collapsed ? 0 : (body?.getBoundingClientRect().height ?? 0)) +
        3,
    );
    const ceiling = Math.max(
      MIN_HEIGHT,
      Math.floor(window.screen.availHeight * MAX_SCREEN_SHARE),
    );
    const max_height = Math.min(Math.max(content, MIN_HEIGHT), ceiling);
    const height =
      collapsed || chosen_height === null
        ? max_height
        : Math.min(Math.max(chosen_height, MIN_HEIGHT), max_height);
    content_height = content;
    overflowing = content > height;
    if (
      max_height === fitted_height &&
      window.innerHeight === height &&
      collapsed === fitted_collapsed
    ) {
      return;
    }
    fitted_height = max_height;
    fitted_collapsed = collapsed;
    const min_height = collapsed ? height : MIN_HEIGHT;
    const resizable = !collapsed;
    pending_resizes += 1;
    resizing = resizing
      .then(async () => {
        await float_window.setResizable(false);
        await float_window.setSizeConstraints({
          minWidth: MIN_WIDTH,
          minHeight: min_height,
          maxHeight: max_height,
        });
        await float_window.setSize(new LogicalSize(window.innerWidth, height));
        if (resizable) {
          await float_window.setResizable(true);
        }
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
      const next_board_name =
        state.boards.find((board) => board.id === ref.boardId)?.name ?? "";
      const snapshot = JSON.stringify([next_board_name, found]);
      if (snapshot !== loaded_snapshot) {
        loaded_snapshot = snapshot;
        board_name = next_board_name;
        task = found?.task ?? null;
        column_name = found?.column ?? "";
        missing = found === null;
      }
      error = null;
    } catch (caught) {
      if (generation !== load_generation) {
        return;
      }
      const parsed = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
      if (parsed.code === "BOARD_NOT_FOUND") {
        loaded_snapshot = "";
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

  // The window opens hidden and is shown once the first read and the height
  // fit settle, instead of flashing the loader or an empty window. A hidden
  // window may not paint frames, so this does not wait for one.
  function announce_ready() {
    void tick()
      .then(() => {
        window.dispatchEvent(new Event("cardbe:workspace-ready"));
        return resizing;
      })
      .then(() => float_window.show())
      .catch((caught) => logger.warn("floating_task.show.failed", caught));
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
    const clock = window.setInterval(
      () => (now = new Date()),
      CLOCK_INTERVAL_MS,
    );
    const handle_focus = () => void load();
    window.addEventListener("focus", handle_focus);
    let reload_timer: number | undefined;
    const data_changed = listen("cardbe:data-changed", () => {
      window.clearTimeout(reload_timer);
      reload_timer = window.setTimeout(() => void load(), RELOAD_DELAY_MS);
    });
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
      window.removeEventListener("wheel", handle_wheel);
      window.clearTimeout(zoom_indicator_timer);
      window.clearInterval(clock);
      window.clearTimeout(reload_timer);
      window.removeEventListener("focus", handle_focus);
      void data_changed.then((unlisten) => unlisten());
    };
  });

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

<FloatingWindow
  title={window_title}
  busy={loading}
  hint={m.floating_task_hint()}
  open_label={m.floating_task_open_in_cardbe()}
  can_open={!!task}
  close_label={m.floating_task_close()}
  log_scope="floating_task"
  onOpen={() => void open_in_cardbe()}
  onClose={() => void close_floating_window()}
>
  <div class="flex min-h-0 flex-1 flex-col">
    <header
      class={collapsed
        ? "relative flex min-h-10 shrink-0 items-center px-3 pe-12"
        : "relative shrink-0 px-3 pt-2.5 pb-1.5 pe-12"}
      bind:this={header}
    >
      <h1
        class={collapsed
          ? "min-w-0 flex-1 truncate text-sm leading-5 font-semibold"
          : "line-clamp-2 text-sm leading-5 font-semibold [overflow-wrap:anywhere]"}
        style:color={task ? display_task_color(task.color) : undefined}
      >
        {task?.title ?? (loading ? "" : m.task_missing())}
      </h1>
      {#if !collapsed}
        {#if task}
          <p class="truncate text-xs text-muted-foreground">
            {board_name} · {column_name}
          </p>
        {/if}
      {/if}
      <Button
        variant="ghost"
        size="icon-xs"
        class="absolute top-2 right-3"
        aria-label={collapsed
          ? m.floating_window_restore()
          : m.floating_window_collapse()}
        title={collapsed
          ? m.floating_window_restore()
          : m.floating_window_collapse()}
        aria-expanded={!collapsed}
        onclick={toggle_collapsed}
      >
        {#if collapsed}<Maximize2Icon />{:else}<MinusIcon />{/if}
      </Button>
    </header>

    {#if !collapsed && !title_only}
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
            {#if task.due_time || task.recurrence || task.priority}
              <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                {#if task.due_time}
                  <DueBadge
                    due_time={task.due_time}
                    {now}
                    at_midnight={task.due_at_midnight}
                  />
                {/if}
                {#if task.recurrence}
                  <span
                    class="inline-flex items-center gap-1.5 text-muted-foreground"
                  >
                    <Repeat2Icon class="size-3.5 shrink-0" />
                    <span>{recurrence_label(task.recurrence)}</span>
                  </span>
                {/if}
                {#if task.priority}
                  <PriorityIndicator priority={task.priority} />
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
</FloatingWindow>
