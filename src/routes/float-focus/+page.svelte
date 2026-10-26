<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "#lib/command-errors.js";
  import {
    decode_floating_focus_id,
    forget_floating_focus,
    resize_floating_focus,
    read_floating_focus_state,
    write_floating_focus_state,
    DAILY_PLAN_ID,
  } from "#lib/floating-focuses.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { logger } from "#lib/logger.js";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, tick } from "svelte";

  import { LogicalSize } from "@tauri-apps/api/dpi";
  import MinusIcon from "@lucide/svelte/icons/minus";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import ListFilterIcon from "@lucide/svelte/icons/list-filter";

  import { Button } from "#lib/components/ui/button/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import type { AllTaskItem, BoardSummary } from "../board-base.svelte";
  import { create_focus, type Focus } from "../type/focus";
  import DailyPlan from "../components/home/daily_plan.svelte";
  import SunIcon from "@lucide/svelte/icons/sun";
  import { get_task_id } from "../type/task.svelte";
  import PriorityIndicator from "../components/task/priority_indicator.svelte";
  import DueBadge from "../components/task/due_badge.svelte";
  import FloatingWindow from "../components/floating/floating_window.svelte";
  import { group_focus_items } from "../components/focus/focus-groups";
  import { load_focus_results } from "../components/focus/focus-results";

  // Writes announce themselves with `cardbe:data-changed`. Due-date filters
  // and groups depend on the time, so the list also reloads each minute.
  const CLOCK_INTERVAL_MS = 60_000;
  // One edit can write several times; reload once after the burst.
  const RELOAD_DELAY_MS = 100;
  // A glanceable list; the full view stays in the main window.
  const PAGE_SIZE = 100;

  const view_id = decode_floating_focus_id(
    new URLSearchParams(window.location.search).get("focus"),
  );
  const float_window = getCurrentWindow();
  const daily_plan = view_id === DAILY_PLAN_ID;

  const saved_window_state = read_floating_focus_state(view_id ?? "");
  let collapsed = $state(saved_window_state.collapsed);
  let resizing = $state(false);
  let expanded_size = new LogicalSize(320, saved_window_state.expanded_height);

  async function toggle_collapsed() {
    if (resizing) {
      return;
    }
    resizing = true;
    const next = !collapsed;
    try {
      const size = await float_window.innerSize();
      const current_size = size.toLogical(await float_window.scaleFactor());
      expanded_size = new LogicalSize(
        current_size.width,
        next ? Math.max(120, current_size.height) : expanded_size.height,
      );
      collapsed = next;
      await tick();
      await resize_floating_focus(float_window, next, expanded_size);
      if (view_id !== null) {
        write_floating_focus_state(view_id, next, expanded_size.height);
      }
    } catch (caught) {
      collapsed = !next;
      logger.warn("floating_focus.resize.failed", caught);
      error = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
      await resize_floating_focus(float_window, collapsed, expanded_size).catch(
        (failure) => logger.warn("floating_focus.resize.failed", failure),
      );
    } finally {
      resizing = false;
    }
  }

  let view = $state<Focus | null>(null);
  let boards = $state<BoardSummary[]>([]);
  let items = $state<AllTaskItem[]>([]);
  let daily_plan_count = $state<number | null>(null);
  const task_count = $derived(daily_plan ? daily_plan_count : items.length);
  let has_more = $state(false);
  let loading = $state(true);
  let error = $state<CommandError | null>(null);
  let now = $state(new Date());
  let busy_ids = $state(new Set<string>());
  let load_generation = 0;
  // Unchanged reads keep the rendered list, so writes elsewhere do not
  // re-render it.
  let loaded_snapshot = "";

  const groups = $derived(
    view ? group_focus_items(items, view.group, now) : [],
  );

  const window_title = $derived(
    daily_plan
      ? `${m.daily_plan_title()} · Cardbe`
      : view
        ? `${view.name} · Cardbe`
        : "Cardbe",
  );

  function item_key(item: AllTaskItem) {
    return `${item.board_id}:${item.archived_at ? "a" : ""}${item.task.id}`;
  }

  function writable(item: AllTaskItem) {
    return (
      boards.find((board) => board.id === item.board_id)?.shared_role !==
      "viewer"
    );
  }

  async function load() {
    const generation = ++load_generation;
    try {
      const [views, state] = await Promise.all([
        daily_plan
          ? Promise.resolve(null)
          : invoke<Focus[] | null>("get_focuses"),
        invoke<{ boards: BoardSummary[] }>("get_boards"),
      ]);
      if (generation !== load_generation) {
        return;
      }
      const found = daily_plan
        ? { ...create_focus(m.daily_plan_title()), id: DAILY_PLAN_ID }
        : views?.find((candidate) => candidate.id === view_id);
      if (!found) {
        // The view was deleted, here or by restoring a backup.
        void close_floating_window();
        return;
      }
      const loaded_at = new Date();
      const page = daily_plan
        ? { items: [], next_cursor: null }
        : await load_focus_results(found, loaded_at, null, PAGE_SIZE);
      if (generation !== load_generation) {
        return;
      }
      const snapshot = JSON.stringify([
        found,
        state.boards,
        page.items,
        page.next_cursor !== null,
      ]);
      if (snapshot !== loaded_snapshot) {
        loaded_snapshot = snapshot;
        view = found;
        boards = state.boards;
        items = page.items;
        has_more = page.next_cursor !== null;
      }
      now = loaded_at;
      error = null;
    } catch (caught) {
      if (generation === load_generation) {
        logger.warn("floating_focus.load.failed", caught);
        error = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
      }
    } finally {
      if (generation === load_generation && loading) {
        loading = false;
        // The window opens hidden and is shown once the first read settles.
        // A hidden window may not paint frames, so this does not wait for one.
        void tick()
          .then(async () => {
            if (collapsed) {
              const size = await float_window.innerSize();
              const logical_size = size.toLogical(
                await float_window.scaleFactor(),
              );
              expanded_size.width = logical_size.width;
              await resize_floating_focus(float_window, true, expanded_size);
            }
            window.dispatchEvent(new Event("cardbe:workspace-ready"));
            return float_window.show();
          })
          .catch((caught) => logger.warn("floating_focus.show.failed", caught));
      }
    }
  }

  async function complete(item: AllTaskItem) {
    const key = item_key(item);
    if (busy_ids.has(key)) {
      return;
    }
    busy_ids = new Set(busy_ids).add(key);
    try {
      await invoke("complete_board_tasks", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
      });
    } catch (caught) {
      logger.warn("floating_focus.complete.failed", caught);
      error = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
    } finally {
      const next = new Set(busy_ids);
      next.delete(key);
      busy_ids = next;
      void load();
    }
  }

  async function open_task(item: AllTaskItem) {
    try {
      await invoke("open_task_in_main_window", {
        boardId: item.board_id,
        taskId: get_task_id(item.task.id),
      });
    } catch (caught) {
      logger.warn("floating_focus.open_task.failed", caught);
      error = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
    }
  }

  async function open_in_cardbe() {
    try {
      await invoke("open_focus_in_main_window", { id: view_id });
    } catch (caught) {
      logger.warn("floating_focus.open_main.failed", caught);
      error = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
    }
  }

  onMount(() => {
    if (view_id === null) {
      void close_floating_window();
      return;
    }
    void load();
    const clock = window.setInterval(() => void load(), CLOCK_INTERVAL_MS);
    const handle_focus = () => void load();
    window.addEventListener("focus", handle_focus);
    let reload_timer: number | undefined;
    const reload_soon = () => {
      window.clearTimeout(reload_timer);
      reload_timer = window.setTimeout(() => void load(), RELOAD_DELAY_MS);
    };
    const unlisteners = [
      listen("cardbe:data-changed", reload_soon),
      listen("cardbe:focuses-changed", reload_soon),
    ];
    return () => {
      window.clearInterval(clock);
      window.clearTimeout(reload_timer);
      window.removeEventListener("focus", handle_focus);
      for (const unlisten of unlisteners) {
        void unlisten.then((stop) => stop());
      }
    };
  });

  async function close_floating_window() {
    if (view_id !== null) {
      forget_floating_focus(view_id);
    }
    try {
      await float_window.close();
    } catch (caught) {
      logger.warn("floating_focus.close.failed", caught);
    }
  }
</script>

<FloatingWindow
  title={window_title}
  busy={loading}
  hint={m.focus_float_hint()}
  open_label={m.floating_task_open_in_cardbe()}
  can_open={!!view}
  close_label={daily_plan ? m.daily_plan_float_close() : m.focus_float_close()}
  log_scope="floating_focus"
  onOpen={() => void open_in_cardbe()}
  onClose={() => void close_floating_window()}
>
  <header
    class={[
      "flex h-10 shrink-0 items-center gap-1.5 border-b px-3",
      collapsed && "border-transparent",
    ]}
  >
    {#if daily_plan}<SunIcon
        class="size-3.5 shrink-0 text-muted-foreground"
      />{:else}<ListFilterIcon
        class="size-3.5 shrink-0 text-muted-foreground"
      />{/if}
    <h1 class="min-w-0 flex-1 truncate text-sm leading-5 font-semibold">
      {daily_plan ? m.daily_plan_title() : (view?.name ?? "")}
    </h1>
    {#if view && task_count !== null}
      <span
        class="shrink-0 text-xs text-muted-foreground tabular-nums"
        title={`${m.focus_float_count({ count: formatNumber(task_count) })}${has_more ? m.focus_float_partial() : ""}`}
        >{formatNumber(task_count)}{has_more ? "+" : ""}</span
      >
    {/if}
    <Button
      variant="ghost"
      size="icon-xs"
      aria-label={collapsed
        ? m.floating_window_restore()
        : m.floating_window_collapse()}
      title={collapsed
        ? m.floating_window_restore()
        : m.floating_window_collapse()}
      aria-expanded={!collapsed}
      disabled={loading || resizing}
      onclick={() => void toggle_collapsed()}
    >
      {#if collapsed}<Maximize2Icon />{:else}<MinusIcon />{/if}
    </Button>
  </header>

  {#if !collapsed || daily_plan}
    <ScrollArea
      orientation="vertical"
      class={collapsed ? "hidden" : "min-h-0 flex-1"}
    >
      <div class="space-y-2 p-1.5">
        {#if error}
          <p class="px-1.5 text-xs text-destructive" aria-live="polite">
            {translateCommandError(error)}
          </p>
        {/if}
        {#if daily_plan && view}
          <DailyPlan
            floating
            {boards}
            onCountChange={(count) => (daily_plan_count = count)}
            onOpenTask={(item) => void open_task(item)}
          />
        {:else}
          {#if view && items.length === 0 && !error}
            <p class="px-1.5 py-4 text-center text-xs text-muted-foreground">
              {m.focus_empty()}
            </p>
          {/if}
          {#each groups as group (group.key)}
            <section aria-label={group.label ?? view?.name}>
              {#if group.label !== null}
                <h2
                  class="flex items-center gap-1.5 px-1.5 pt-1 pb-0.5 text-xs font-medium text-muted-foreground"
                >
                  <span class="truncate">{group.label}</span>
                  <span class="tabular-nums"
                    >{formatNumber(group.items.length)}</span
                  >
                </h2>
              {/if}
              <ul>
                {#each group.items as item (item_key(item))}
                  {@const finished = item.done || Boolean(item.archived_at)}
                  <li
                    class="flex items-center gap-1 rounded-md hover:bg-muted/50"
                  >
                    {#if !finished && writable(item)}
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        class="shrink-0 text-muted-foreground"
                        disabled={busy_ids.has(item_key(item))}
                        aria-label={m.task_complete()}
                        title={m.task_complete()}
                        onclick={() => void complete(item)}
                      >
                        <CircleCheckIcon />
                      </Button>
                    {:else}
                      <span class="size-8 shrink-0"></span>
                    {/if}
                    <button
                      type="button"
                      class="min-w-0 flex-1 rounded-sm py-1 pe-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      title={m.floating_task_open_in_cardbe()}
                      onclick={() => void open_task(item)}
                    >
                      <span
                        class={[
                          "block truncate text-sm",
                          finished && "text-muted-foreground line-through",
                        ]}>{item.task.title || m.task_untitled()}</span
                      >
                      <span
                        class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground"
                      >
                        <span class="truncate">{item.board_name}</span>
                        {#if item.task.priority}
                          <PriorityIndicator priority={item.task.priority} />
                        {/if}
                        {#if item.task.due_time}
                          <DueBadge
                            due_time={item.task.due_time}
                            {now}
                            at_midnight={item.task.due_at_midnight}
                          />
                        {/if}
                      </span>
                    </button>
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
          {#if has_more}
            <Button
              variant="link"
              size="sm"
              class="w-full"
              onclick={() => void open_in_cardbe()}
              >{m.focus_float_show_all()}</Button
            >
          {/if}
        {/if}
      </div>
    </ScrollArea>
  {/if}
</FloatingWindow>
