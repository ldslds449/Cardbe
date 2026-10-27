<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { logger } from "#lib/logger.js";
  import { listen } from "@tauri-apps/api/event";
  import { untrack, type Snippet } from "svelte";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Empty from "#lib/components/ui/empty/index.js";
  import { Spinner } from "#lib/components/ui/spinner/index.js";

  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";

  import type { AllTaskItem } from "../../board-base.svelte";
  import type { FocusGroup } from "../../type/focus";
  import {
    group_focus_items,
    type FocusGroupItems,
  } from "../focus/focus-groups";
  import type { FocusResults } from "../focus/focus-results";
  import { task_item_key, TaskActions } from "./task-actions.svelte";
  import TaskRow, { type TaskRowBoard } from "./task_row.svelte";
  import TaskRowMenus from "./task_row_menus.svelte";

  const PAGE_SIZE = 100;
  // Rows drawn per frame, so a page appears without holding up the switch.
  const RENDER_BATCH = 25;
  // How often an open list checks whether the day has changed.
  const DAY_CHECK_MS = 60_000;

  let {
    load_key,
    load,
    group,
    label,
    boards,
    paused = false,
    highlight = "",
    count = $bindable(0),
    has_more = $bindable(false),
    empty,
    onOpenTask,
    onEditTask,
    onArchiveTask,
    onOpenBoard,
  }: {
    /** Changes whenever `load` would read different tasks. */
    load_key: string;
    /** Reads one page, with relative dates taken at `now`. */
    load: (
      now: Date,
      cursor: string | null,
      limit: number,
    ) => Promise<FocusResults>;
    group: FocusGroup;
    /** Names the list when it is not grouped. */
    label: string;
    boards: TaskRowBoard[];
    /** Set while a dialog opened from this list may still change tasks. */
    paused?: boolean;
    /** Search text to mark in titles. */
    highlight?: string;
    /** Loaded tasks, for the caller's summary. */
    count?: number;
    has_more?: boolean;
    /** Shown when nothing matches. */
    empty: Snippet;
    onOpenTask: (item: AllTaskItem) => unknown;
    onEditTask: (item: AllTaskItem) => unknown;
    onArchiveTask: (item: AllTaskItem) => unknown;
    onOpenBoard: (board_id: number) => void;
  } = $props();

  let items = $state<AllTaskItem[]>([]);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let failed = $state(false);
  let now = $state(new Date());
  let request = 0;
  const actions = new TaskActions(() => refresh());

  let menus = $state<ReturnType<typeof TaskRowMenus> | null>(null);
  let render_limit = $state(RENDER_BATCH);
  // Groups name their full size while their rows are still being drawn.
  let group_sizes = $derived(
    new Map(
      group_focus_items(items, group, now).map((entry) => [
        entry.key,
        entry.items.length,
      ]),
    ),
  );
  let groups = $derived(
    group_focus_items(items.slice(0, render_limit), group, now),
  );
  const board_by_id = $derived(
    new Map(boards.map((board) => [board.id, board])),
  );
  // The key the shown tasks belong to; other tasks stay hidden while
  // switching.
  let loaded_key = $state<string | null>(null);

  $effect(() => {
    count = loaded_key === load_key ? items.length : 0;
    has_more = loaded_key === load_key && cursor !== null;
  });

  async function load_page(reset: boolean) {
    const current = ++request;
    const key = load_key;
    if (reset) {
      now = new Date();
    }
    loading = true;
    failed = false;
    try {
      const page = await load(now, reset ? null : cursor, PAGE_SIZE);
      if (current !== request) {
        return;
      }
      if (key !== loaded_key) {
        render_limit = RENDER_BATCH;
      }
      items = reset ? page.items : [...items, ...page.items];
      cursor = page.next_cursor;
      loaded_key = key;
    } catch (error) {
      if (current === request) {
        logger.error("focus.load.failed", error);
        failed = true;
      }
    } finally {
      if (current === request) {
        loading = false;
      }
    }
  }

  $effect(() => {
    if (render_limit >= items.length) {
      return;
    }
    const frame = requestAnimationFrame(() => (render_limit += RENDER_BATCH));
    return () => cancelAnimationFrame(frame);
  });

  function refresh() {
    void load_page(true);
  }

  // Reload when the conditions change or a dialog closes. Loading reads and
  // writes its own state, which must not re-run this effect.
  $effect(() => {
    void load_key;
    if (!paused) {
      untrack(refresh);
    }
  });

  // Every saved change reloads the list, so only time can make it stale:
  // overdue and due-today tasks change at midnight.
  $effect(() => {
    const unlisten = listen("cardbe:data-changed", () => {
      if (!paused) {
        refresh();
      }
    });
    const timer = setInterval(() => {
      if (!paused && new Date().toDateString() !== now.toDateString()) {
        refresh();
      }
    }, DAY_CHECK_MS);
    return () => {
      clearInterval(timer);
      void unlisten.then((stop) => stop());
    };
  });
</script>

{#snippet task_list(entries: AllTaskItem[], name: string)}
  <ul class="space-y-2" aria-label={name}>
    {#each entries as item (task_item_key(item))}
      <li>
        <TaskRow
          {item}
          density="full"
          board={board_by_id.get(item.board_id)}
          {now}
          {highlight}
          {actions}
          {onOpenTask}
          onMenu={(entry, anchor) => menus?.open_menu(entry, anchor)}
        />
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet group_section(entry: FocusGroupItems)}
  {#if entry.label === null}
    {@render task_list(entry.items, label)}
  {:else}
    <section class="rounded-xl border bg-muted/20 p-4" aria-label={entry.label}>
      <header class="mb-3 flex items-start justify-between gap-3">
        <h3 class="min-w-0 truncate font-semibold tracking-tight">
          {entry.label}
        </h3>
        <Badge
          variant={entry.key === "overdue" && entry.items.length > 0
            ? "destructive"
            : "secondary"}
          class={entry.key === "today" && entry.items.length > 0
            ? "border-warning/30 bg-warning/10 text-warning"
            : ""}
        >
          {formatNumber(group_sizes.get(entry.key) ?? entry.items.length)}
        </Badge>
      </header>
      {@render task_list(entry.items, entry.label)}
    </section>
  {/if}
{/snippet}

<svelte:window
  onfocus={() => {
    if (!paused) {
      refresh();
    }
  }}
/>

{#if failed && (items.length === 0 || loaded_key !== load_key)}
  <Empty.Root>
    <Empty.Header>
      <Empty.Media variant="icon"><CircleAlertIcon /></Empty.Media>
      <Empty.Title>{m.focus_load_error()}</Empty.Title>
    </Empty.Header>
    <Empty.Content>
      <Button onclick={refresh}>{m.common_retry()}</Button>
    </Empty.Content>
  </Empty.Root>
{:else if loaded_key !== load_key}
  <!-- Shown only when loading is slow enough to notice. -->
  <div
    class="flex animate-in justify-center py-16 delay-300 fill-mode-backwards fade-in-0"
    role="status"
  >
    <Spinner />
    <span class="sr-only">{m.focus_loading()}</span>
  </div>
{:else if items.length === 0}
  {@render empty()}
{:else}
  <TaskRowMenus
    bind:this={menus}
    class="block space-y-5"
    {items}
    boards={board_by_id}
    {actions}
    {onOpenTask}
    {onEditTask}
    {onArchiveTask}
    {onOpenBoard}
  >
    {#each groups as entry (entry.key)}
      {@render group_section(entry)}
    {/each}
  </TaskRowMenus>
  {#if cursor}
    <div class="mt-5 flex justify-center">
      <Button
        variant="outline"
        disabled={loading}
        onclick={() => void load_page(false)}>{m.focus_load_more()}</Button
      >
    </div>
  {/if}
{/if}
