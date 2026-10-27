<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import { untrack } from "svelte";
  import ListTodoIcon from "@lucide/svelte/icons/list-todo";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import SearchIcon from "@lucide/svelte/icons/search";
  import XIcon from "@lucide/svelte/icons/x";

  import * as Empty from "#lib/components/ui/empty/index.js";
  import * as InputGroup from "#lib/components/ui/input-group/index.js";
  import * as Sheet from "#lib/components/ui/sheet/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { Spinner } from "#lib/components/ui/spinner/index.js";

  import type { AllTaskItem, BoardSummary } from "../../board.svelte";
  import type { TaskExplorerQuery } from "../../utils/task-explorer";
  import VirtualList from "../virtual_list.svelte";
  import BoardScopeMenu from "./board_scope_menu.svelte";
  import { task_item_key, TaskActions } from "./task-actions.svelte";
  import {
    board_scope_name,
    TaskFilters,
    type TaskFilterValues,
  } from "./task-filters.svelte";
  import { task_counts_key, TaskCounts } from "./task-counts.svelte";
  import TaskStatusButtons from "./task_status_buttons.svelte";
  import TaskRow from "./task_row.svelte";
  import TaskRowMenus from "./task_row_menus.svelte";

  let {
    open = $bindable(false),
    boards,
    request,
    all_task_items,
    all_task_items_loading = false,
    all_task_items_has_more = false,
    all_task_items_error = false,
    onLoadMoreAllTasks,
    onSearchAllTasks,
    onOpenTask,
    onEditTask,
    onArchiveTask,
    onOpenBoard,
    onOpenFullPage,
  }: {
    open: boolean;
    boards: BoardSummary[];
    /** Opens the explorer searching for `text`; each new object applies once. */
    request?: { text: string } | null;
    all_task_items: AllTaskItem[];
    all_task_items_loading?: boolean;
    all_task_items_has_more?: boolean;
    all_task_items_error?: boolean;
    onLoadMoreAllTasks: () => void;
    onSearchAllTasks: (filter: TaskExplorerQuery) => void;
    onOpenTask: (item: AllTaskItem) => unknown;
    onEditTask: (item: AllTaskItem) => unknown;
    onArchiveTask: (item: AllTaskItem) => unknown;
    onOpenBoard: (board_id: number) => void;
    /** Continues browsing these conditions on the All tasks page. */
    onOpenFullPage: (values: TaskFilterValues) => void;
  } = $props();

  // Finding a task needs only search, scope, and status; the All tasks page
  // offers the other filters.
  const filters = new TaskFilters();
  const counts = new TaskCounts();
  const actions = new TaskActions(() => {
    onSearchAllTasks(query);
    void counts.load(filters.values);
  });
  const scope_name = $derived(board_scope_name(filters.board_ids, boards));
  const board_by_id = $derived(
    new Map(boards.map((board) => [board.id, board])),
  );
  let now = $state(new Date());
  let menus = $state<ReturnType<typeof TaskRowMenus> | null>(null);
  let requested_query = $state("");

  const query = $derived(filters.query(now));
  const query_key = $derived(JSON.stringify(query));
  const pending = $derived(requested_query !== query_key);
  const entries: AllTaskItem[] = $derived(pending ? [] : all_task_items);
  const loading = $derived(pending || all_task_items_loading);
  const has_more = $derived(!pending && all_task_items_has_more);
  const counts_key = $derived(task_counts_key(filters.values));
  const row_height = 72;
  const empty_title = $derived(
    filters.board_ids?.length === 0
      ? m.explorer_no_boards()
      : filters.searching
        ? m.explorer_no_matching()
        : filters.status === "all"
          ? m.explorer_no_tasks()
          : {
              active: m.explorer_empty_active,
              overdue: m.explorer_empty_overdue,
              recurring: m.explorer_empty_recurring,
              archived: m.explorer_empty_archived,
            }[filters.status](),
  );

  $effect(() => {
    if (!open) {
      return;
    }
    now = new Date();
  });

  $effect(() => {
    // An empty board search keeps the explorer's last search.
    if (request?.text) {
      filters.search_text = request.text;
    }
  });

  $effect(() => {
    if (!open) {
      return;
    }
    const key = query_key;
    const filter = query;
    const timer = setTimeout(() => {
      requested_query = key;
      onSearchAllTasks(filter);
    }, 160);
    return () => clearTimeout(timer);
  });

  // Changing only the status keeps the counts.
  $effect(() => {
    if (!open) {
      return;
    }
    void counts_key;
    const timer = setTimeout(
      () => void counts.load(untrack(() => filters.values)),
      160,
    );
    return () => clearTimeout(timer);
  });

  function open_full_page() {
    onOpenFullPage(filters.values);
    open = false;
  }
</script>

<Sheet.Root bind:open>
  <Sheet.Content
    class="flex h-full flex-col gap-0"
    side="right"
    preventScroll={false}
    resizable
    defaultWidth={720}
    minWidth={440}
    maxWidth={1100}
    widthStorageKey="cardbe.all-task-explorer-width"
    onOpenAutoFocus={(event) => {
      event.preventDefault();
      document.getElementById("task-explorer-search")?.focus();
    }}
  >
    <Sheet.Header class="border-b px-6 pb-3 pt-4">
      <div class="flex items-center justify-between gap-4 pr-8">
        <div class="min-w-0">
          <Sheet.Title class="flex items-center gap-2">
            <ListTodoIcon class="size-5 text-primary" />
            {m.task_explorer()}
          </Sheet.Title>
          <Sheet.Description class="sr-only"
            >{m.explorer_description()}</Sheet.Description
          >
        </div>
        <Button
          variant="ghost"
          size="sm"
          class="shrink-0"
          title={m.explorer_open_full_page_hint()}
          onclick={open_full_page}
        >
          <Maximize2Icon class="size-4" />{m.explorer_open_full_page()}
        </Button>
      </div>
    </Sheet.Header>

    <div class="grid shrink-0 gap-3 border-b px-6 py-3">
      <div class="flex items-end gap-3">
        <InputGroup.Root class="min-w-0 flex-[2]">
          <InputGroup.Input
            id="task-explorer-search"
            placeholder={filters.board_ids === null
              ? m.explorer_search_all()
              : m.explorer_search_selected()}
            aria-label={filters.board_ids === null
              ? m.explorer_search_all()
              : m.explorer_search_selected()}
            bind:value={filters.search_text}
          />
          <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
          {#if filters.search_text}
            <InputGroup.Addon align="inline-end">
              <InputGroup.Button
                aria-label={m.explorer_clear_search()}
                size="icon-xs"
                onclick={() => (filters.search_text = "")}
              >
                <XIcon />
              </InputGroup.Button>
            </InputGroup.Addon>
          {/if}
        </InputGroup.Root>
        <div class="min-w-[9rem] flex-1">
          <span id="task-explorer-board-label" class="sr-only"
            >{m.explorer_scope()}</span
          >
          <BoardScopeMenu
            id="task-explorer-board"
            {boards}
            board_ids={filters.board_ids}
            onChange={(board_ids) => filters.set_board_ids(board_ids)}
          />
        </div>
      </div>
      <TaskStatusButtons {filters} counts={counts.value} />
      <div aria-live="polite" class="text-xs text-muted-foreground">
        {counts.value
          ? m.task_count({ count: counts.value[filters.status] })
          : has_more
            ? m.explorer_loaded({ count: entries.length })
            : m.task_count({ count: entries.length })}
      </div>
    </div>

    <div class="flex min-h-0 flex-1 flex-col px-4 py-3">
      {#if all_task_items_error && !loading && entries.length === 0}
        <Empty.Root class="h-full border-none">
          <Empty.Header
            ><Empty.Title>{m.explorer_load_error()}</Empty.Title>
            <Empty.Description>{m.explorer_retry_hint()}</Empty.Description
            ></Empty.Header
          >
          <Button variant="outline" onclick={() => onSearchAllTasks(query)}
            >{m.common_retry()}</Button
          >
        </Empty.Root>
      {:else if loading && entries.length === 0}
        <div
          class="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground"
        >
          <Spinner />
          {m.explorer_loading()}
        </div>
      {:else if entries.length === 0 && !has_more}
        <Empty.Root class="h-full border-none py-12">
          <Empty.Header>
            <Empty.Media variant="icon"><ListTodoIcon /></Empty.Media>
            <Empty.Title>{empty_title}</Empty.Title>
            <Empty.Description
              >{filters.board_ids?.length === 0
                ? m.explorer_select_board_hint()
                : filters.searching
                  ? m.explorer_search_hint()
                  : m.explorer_empty_scope({
                      name: scope_name,
                    })}</Empty.Description
            >
            {#if filters.searching}
              <Button
                variant="outline"
                onclick={() => (filters.search_text = "")}
                >{m.explorer_clear_search()}</Button
              >
            {/if}
          </Empty.Header>
        </Empty.Root>
      {:else}
        <TaskRowMenus
          bind:this={menus}
          class="flex min-h-0 flex-1 flex-col"
          items={entries}
          boards={board_by_id}
          {actions}
          {onOpenTask}
          {onEditTask}
          {onArchiveTask}
          {onOpenBoard}
        >
          <VirtualList
            items={entries}
            getItemKey={task_item_key}
            rowHeight={row_height}
            class="min-h-0 flex-1"
            hasMore={has_more && !all_task_items_error}
            {loading}
            resetKey={query_key}
            onLoadMore={onLoadMoreAllTasks}
          >
            {#snippet children(item: AllTaskItem, _index: number)}
              <TaskRow
                {item}
                density="compact"
                board={board_by_id.get(item.board_id)}
                {now}
                highlight={filters.search_text}
                {actions}
                {onOpenTask}
                onMenu={(entry, anchor) => menus?.open_menu(entry, anchor)}
              />
            {/snippet}
          </VirtualList>
        </TaskRowMenus>
        {#if loading}
          <div
            class="flex items-center justify-center gap-2 py-2 text-xs text-muted-foreground"
          >
            <Spinner />{m.explorer_loading_more()}
          </div>
        {/if}
        {#if all_task_items_error && !loading}
          <Button
            variant="outline"
            class="shrink-0"
            onclick={onLoadMoreAllTasks}>{m.explorer_retry_more()}</Button
          >
        {/if}
      {/if}
    </div>
  </Sheet.Content>
</Sheet.Root>
