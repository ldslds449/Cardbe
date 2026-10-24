<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "$lib/command-errors";
  import {
    decode_floating_focus_id,
    forget_floating_focus,
  } from "$lib/floating-focuses";
  import { formatNumber } from "$lib/i18n";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { Menu } from "@tauri-apps/api/menu";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import ListFilterIcon from "@lucide/svelte/icons/list-filter";

  import { Button } from "$lib/components/ui/button/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import type { AllTaskItem, BoardSummary } from "../board-base.svelte";
  import type { Focus } from "../type/focus";
  import { get_task_id } from "../type/task.svelte";
  import PriorityIndicator from "../components/task/priority_indicator.svelte";
  import { due_label, due_label_class } from "../components/task/due-label";
  import { group_focus_items } from "../components/focus/focus-groups";
  import { load_focus_results } from "../components/focus/focus-results";

  // Board edits in the main window send no change notifications, so a light
  // periodic read keeps the list current, as in floating tasks.
  const REFRESH_INTERVAL_MS = 5000;
  // A glanceable list; the full view stays in the main window.
  const PAGE_SIZE = 100;

  const view_id = decode_floating_focus_id(
    new URLSearchParams(window.location.search).get("focus"),
  );
  const float_window = getCurrentWindow();

  let view = $state<Focus | null>(null);
  let boards = $state<BoardSummary[]>([]);
  let items = $state<AllTaskItem[]>([]);
  let has_more = $state(false);
  let loading = $state(true);
  let error = $state<CommandError | null>(null);
  let now = $state(new Date());
  let busy_ids = $state(new Set<string>());
  let load_generation = 0;

  const groups = $derived(
    view ? group_focus_items(items, view.group, now) : [],
  );

  const window_title = $derived(view ? `${view.name} · Cardbe` : "Cardbe");

  $effect(() => {
    void float_window
      .setTitle(window_title)
      .catch((caught) => logger.warn("floating_focus.title.failed", caught));
  });

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
        invoke<Focus[] | null>("get_focuses"),
        invoke<{ boards: BoardSummary[] }>("get_boards"),
      ]);
      if (generation !== load_generation) {
        return;
      }
      const found = views?.find((candidate) => candidate.id === view_id);
      if (!found) {
        // The view was deleted, here or by restoring a backup.
        void close_floating_window();
        return;
      }
      const loaded_at = new Date();
      const page = await load_focus_results(found, loaded_at, null, PAGE_SIZE);
      if (generation !== load_generation) {
        return;
      }
      view = found;
      boards = state.boards;
      items = page.items;
      has_more = page.next_cursor !== null;
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
        // Keeps the compact startup loader until the first read settles.
        requestAnimationFrame(() =>
          window.dispatchEvent(new Event("cardbe:workspace-ready")),
        );
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
    const timer = window.setInterval(() => void load(), REFRESH_INTERVAL_MS);
    const handle_focus = () => void load();
    window.addEventListener("focus", handle_focus);
    const unlisteners = [
      listen("cardbe:data-changed", () => void load()),
      listen("cardbe:focuses-changed", () => void load()),
    ];
    return () => {
      void context_menu?.close();
      window.clearInterval(timer);
      window.removeEventListener("focus", handle_focus);
      for (const unlisten of unlisteners) {
        void unlisten.then((stop) => stop());
      }
    };
  });

  // The header is the drag handle. Like Tauri's own drag region, this uses
  // mousedown, whose detail counts clicks, and lets a double-click through.
  function handle_mousedown(event: MouseEvent) {
    if (event.button !== 0 || event.detail === 2) {
      return;
    }
    void float_window
      .startDragging()
      .catch((caught) => logger.warn("floating_focus.drag.failed", caught));
  }

  // A native menu can extend past the small window. As in floating tasks,
  // nothing touches menus or runs an item's action until the popup returns:
  // doing so while it is open deadlocks the app.
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
            enabled: !!view,
            action: () => after_menu(() => void open_in_cardbe()),
          },
          { item: "Separator" },
          {
            text: m.focus_float_close(),
            action: () => after_menu(() => void close_floating_window()),
          },
        ],
      });
      context_menu = menu;
      await menu.popup(undefined, float_window);
    } catch (caught) {
      logger.warn("floating_focus.menu.failed", caught);
    } finally {
      menu_open = false;
      const action = pending_action;
      pending_action = null;
      action?.();
    }
  }

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

<svelte:head>
  <title>{window_title}</title>
</svelte:head>

<main
  class="flex h-screen select-none flex-col overflow-hidden border bg-background text-foreground"
  aria-busy={loading}
  oncontextmenu={show_context_menu}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header
    class="flex shrink-0 cursor-grab items-center gap-1.5 border-b px-3 py-2 active:cursor-grabbing"
    title={m.focus_float_hint()}
    onmousedown={handle_mousedown}
    ondblclick={() => void open_in_cardbe()}
  >
    <ListFilterIcon class="size-3.5 shrink-0 text-muted-foreground" />
    <h1 class="min-w-0 flex-1 truncate text-sm font-semibold">
      {view?.name ?? ""}
    </h1>
    {#if view && !has_more}
      <span class="shrink-0 text-xs text-muted-foreground tabular-nums"
        >{formatNumber(items.length)}</span
      >
    {/if}
  </header>

  <ScrollArea orientation="vertical" class="min-h-0 flex-1">
    <div class="space-y-2 p-1.5">
      {#if error}
        <p class="px-1.5 text-xs text-destructive" aria-live="polite">
          {translateCommandError(error)}
        </p>
      {/if}
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
              {@const due = item.task.due_time
                ? due_label(item.task.due_time, now, item.task.due_at_midnight)
                : undefined}
              {@const finished = item.done || Boolean(item.archived_at)}
              <li class="flex items-center gap-1 rounded-md hover:bg-muted/50">
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
                    {#if due}
                      <span
                        class="inline-flex items-center gap-1 {due_label_class(
                          due.status,
                        )}"
                      >
                        {#if due.status === "overdue"}
                          <CircleAlertIcon class="size-3.5" />
                        {:else}
                          <ClockIcon class="size-3.5" />
                        {/if}
                        {due.text}
                      </span>
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
    </div>
  </ScrollArea>
</main>
