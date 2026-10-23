<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatNumber } from "$lib/i18n";
  import { translateCommandError } from "$lib/command-errors";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { toast } from "svelte-sonner";
  import { untrack } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";

  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import GaugeIcon from "@lucide/svelte/icons/gauge";
  import HourglassIcon from "@lucide/svelte/icons/hourglass";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import SearchXIcon from "@lucide/svelte/icons/search-x";

  import type { AllTaskItem } from "../../board-base.svelte";
  import type { BoardRole } from "../../board.svelte";
  import {
    deserialize_column,
    get_column_id,
    type Column,
    type ColumnSerialized,
  } from "../../type/column.svelte";
  import {
    deserialize_task_summary,
    get_task_id,
    is_task_priority,
    TASK_PRIORITIES,
    type TaskPriority,
    type TaskSummarySerialized,
  } from "../../type/task.svelte";
  import { smart_view_query, type SmartView } from "../../type/smart-view";
  import ChooseColumnDialog from "../dialog/choose_column_dialog.svelte";
  import PriorityIndicator from "../task/priority_indicator.svelte";
  import { due_label, due_label_class } from "../task/due-label";
  import { group_smart_view_items } from "./smart-view-groups";

  const PAGE_SIZE = 100;

  let {
    view,
    boards,
    paused = false,
    onEdit,
    onOpenTask,
    onOpenBoard,
  }: {
    view: SmartView;
    boards: { id: number; shared_role: BoardRole }[];
    /** Set while a dialog opened from this page may still change tasks. */
    paused?: boolean;
    onEdit: () => void;
    onOpenTask: (item: AllTaskItem) => void;
    onOpenBoard: (board_id: number) => void;
  } = $props();

  interface PageSerialized {
    items: Array<{
      board_id: number;
      board_name: string;
      column_id: number | null;
      column_name: string | null;
      archived_at: number | null;
      done: boolean;
      waiting_count: number;
      task: TaskSummarySerialized;
    }>;
    next_cursor: string | null;
  }

  let items = $state<AllTaskItem[]>([]);
  let cursor = $state<string | null>(null);
  let loading = $state(false);
  let failed = $state(false);
  let now = $state(new Date());
  let request = 0;
  let busy_ids = $state(new Set<string>());

  let groups = $derived(group_smart_view_items(items, view.group, now));
  let view_key = $derived(JSON.stringify(view));

  // Reopening asks where the tasks go, among their board's open columns.
  let reopen_item = $state<AllTaskItem | null>(null);
  let reopen_columns = $state<Column[]>([]);
  let reopen_open = $state(false);

  function item_key(item: AllTaskItem) {
    return `${item.board_id}:${item.archived_at ? "a" : ""}${item.task.id}`;
  }

  function writable(item: AllTaskItem) {
    return (
      boards.find((board) => board.id === item.board_id)?.shared_role !==
      "viewer"
    );
  }

  async function load(reset: boolean) {
    const current = ++request;
    if (reset) {
      now = new Date();
    }
    loading = true;
    failed = false;
    try {
      const page = await invoke<PageSerialized>("list_all_tasks", {
        cursor: reset ? null : cursor,
        filter: smart_view_query(view, now),
        limit: PAGE_SIZE,
      });
      if (current !== request) {
        return;
      }
      const loaded = page.items.map((item) => ({
        board_id: item.board_id,
        board_name: item.board_name,
        column_id:
          item.column_id === null ? undefined : `column_${item.column_id}`,
        column_name: item.column_name ?? undefined,
        archived_at:
          item.archived_at === null ? undefined : new Date(item.archived_at),
        done: item.done,
        waiting_count: item.waiting_count,
        task: deserialize_task_summary(item.task),
      }));
      items = reset ? loaded : [...items, ...loaded];
      cursor = page.next_cursor;
    } catch (error) {
      if (current === request) {
        logger.error("smart_view.load.failed", error);
        failed = true;
      }
    } finally {
      if (current === request) {
        loading = false;
      }
    }
  }

  function refresh() {
    void load(true);
  }

  // Reload when the view changes or a dialog closes. Loading reads and
  // writes its own state, which must not re-run this effect.
  $effect(() => {
    void view_key;
    if (!paused) {
      untrack(refresh);
    }
  });

  $effect(() => {
    const unlisten = listen("cardbe:data-changed", () => {
      if (!paused) {
        refresh();
      }
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  });

  async function act(item: AllTaskItem, action: () => Promise<unknown>) {
    const key = item_key(item);
    if (busy_ids.has(key)) {
      return;
    }
    busy_ids = new Set(busy_ids).add(key);
    try {
      await action();
    } catch (error) {
      toast.error(translateCommandError(error));
    } finally {
      const next = new Set(busy_ids);
      next.delete(key);
      busy_ids = next;
      refresh();
    }
  }

  function complete(item: AllTaskItem) {
    void act(item, () =>
      invoke("complete_board_tasks", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
      }),
    );
  }

  function set_priority(item: AllTaskItem, priority: TaskPriority | undefined) {
    void act(item, () =>
      invoke("set_board_tasks_priority", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
        priority: priority ?? null,
      }),
    );
  }

  async function ask_to_reopen(item: AllTaskItem) {
    try {
      const columns = await invoke<ColumnSerialized[]>("get_board_columns", {
        boardId: item.board_id,
      });
      reopen_columns = columns
        .map(deserialize_column)
        .filter((column) => !column.done);
      reopen_item = item;
      reopen_open = true;
    } catch (error) {
      toast.error(translateCommandError(error));
    }
  }

  function reopen(column_id: string) {
    const item = reopen_item;
    if (!item) {
      return;
    }
    void act(item, () =>
      invoke("reopen_board_tasks", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
        columnId: get_column_id(column_id),
      }),
    );
  }

  function location(item: AllTaskItem) {
    return m.smart_view_location({
      board: item.board_name,
      column: item.archived_at
        ? m.explorer_archived()
        : (item.column_name ?? ""),
    });
  }
</script>

<svelte:window
  onfocus={() => {
    if (!paused) {
      refresh();
    }
  }}
/>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  <header class="flex shrink-0 items-center gap-2 border-b px-5 py-3">
    <h2 class="min-w-0 flex-1 truncate text-base font-semibold">
      {view.name}
    </h2>
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={m.smart_view_refresh()}
      title={m.smart_view_refresh()}
      disabled={loading}
      onclick={refresh}
    >
      <RefreshCwIcon />
    </Button>
    <Button variant="outline" size="sm" onclick={onEdit}>
      <PencilIcon />
      {m.common_edit()}
    </Button>
  </header>

  <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="vertical">
    <div class="mx-auto w-full max-w-4xl p-5">
      {#if failed && items.length === 0}
        <Empty.Root>
          <Empty.Header>
            <Empty.Media variant="icon"><CircleAlertIcon /></Empty.Media>
            <Empty.Title>{m.smart_view_load_error()}</Empty.Title>
          </Empty.Header>
          <Empty.Content>
            <Button onclick={refresh}>{m.common_retry()}</Button>
          </Empty.Content>
        </Empty.Root>
      {:else if loading && items.length === 0}
        <div class="flex justify-center py-16" role="status">
          <Spinner />
          <span class="sr-only">{m.smart_view_loading()}</span>
        </div>
      {:else if items.length === 0}
        <Empty.Root>
          <Empty.Header>
            <Empty.Media variant="icon"><SearchXIcon /></Empty.Media>
            <Empty.Title>{m.smart_view_empty()}</Empty.Title>
            <Empty.Description
              >{m.smart_view_empty_description()}</Empty.Description
            >
          </Empty.Header>
          <Empty.Content>
            <Button variant="outline" onclick={onEdit}>
              <PencilIcon />
              {m.common_edit()}
            </Button>
          </Empty.Content>
        </Empty.Root>
      {:else}
        <div class="space-y-6">
          {#each groups as group (group.key)}
            <section aria-label={group.label ?? view.name}>
              {#if group.label !== null}
                <h3
                  class="mb-2 flex items-center gap-2 text-sm font-medium text-muted-foreground"
                >
                  <span class="truncate">{group.label}</span>
                  <span class="tabular-nums"
                    >{formatNumber(group.items.length)}</span
                  >
                </h3>
              {/if}
              <ul class="divide-y rounded-md border">
                {#each group.items as item (item_key(item))}
                  {@const due = item.task.due_time
                    ? due_label(
                        item.task.due_time,
                        now,
                        item.task.due_at_midnight,
                      )
                    : undefined}
                  {@const can_write = writable(item)}
                  {@const busy = busy_ids.has(item_key(item))}
                  {@const finished = item.done || Boolean(item.archived_at)}
                  <li class="flex items-center gap-2 px-3 py-2">
                    {#if can_write}
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        class={finished
                          ? "text-primary"
                          : "text-muted-foreground"}
                        disabled={busy}
                        aria-label={finished
                          ? m.task_reopen_action()
                          : m.task_complete()}
                        title={finished
                          ? m.task_reopen_action()
                          : m.task_complete()}
                        onclick={() =>
                          finished ? void ask_to_reopen(item) : complete(item)}
                      >
                        {#if finished}
                          <RotateCcwIcon />
                        {:else}
                          <CircleCheckIcon />
                        {/if}
                      </Button>
                    {/if}
                    <button
                      type="button"
                      class="min-w-0 flex-1 rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      onclick={() => onOpenTask(item)}
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
                        <span class="truncate">{location(item)}</span>
                        {#if item.task.priority}
                          <PriorityIndicator priority={item.task.priority} />
                        {/if}
                        {#if item.waiting_count}
                          <span
                            class="inline-flex items-center gap-1"
                            title={m.task_waiting_count({
                              count: item.waiting_count,
                            })}
                          >
                            <HourglassIcon class="size-3.5" />
                            <span aria-hidden="true"
                              >{formatNumber(item.waiting_count)}</span
                            >
                            <span class="sr-only"
                              >{m.task_waiting_count({
                                count: item.waiting_count,
                              })}</span
                            >
                          </span>
                        {/if}
                        {#if due}
                          <span
                            class={`inline-flex items-center gap-1 ${due_label_class(due.status)}`}
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
                    <DropdownMenu.Root>
                      <DropdownMenu.Trigger>
                        {#snippet child({ props })}
                          <Button
                            {...props}
                            variant="ghost"
                            size="icon-sm"
                            aria-label={m.smart_view_task_actions({
                              name: item.task.title || m.task_untitled(),
                            })}
                          >
                            <EllipsisIcon />
                          </Button>
                        {/snippet}
                      </DropdownMenu.Trigger>
                      <DropdownMenu.Content align="end" class="min-w-44">
                        {#if can_write}
                          <DropdownMenu.Sub>
                            <DropdownMenu.SubTrigger disabled={busy}>
                              <GaugeIcon />
                              {m.task_priority()}
                            </DropdownMenu.SubTrigger>
                            <DropdownMenu.SubContent class="min-w-36">
                              <DropdownMenu.RadioGroup
                                value={item.task.priority ?? "none"}
                                onValueChange={(value) =>
                                  set_priority(
                                    item,
                                    is_task_priority(value) ? value : undefined,
                                  )}
                              >
                                {#each [...TASK_PRIORITIES].reverse() as priority (priority)}
                                  <DropdownMenu.RadioItem value={priority}>
                                    <PriorityIndicator {priority} />
                                  </DropdownMenu.RadioItem>
                                {/each}
                                <DropdownMenu.RadioItem value="none"
                                  >{m.task_priority_none()}</DropdownMenu.RadioItem
                                >
                              </DropdownMenu.RadioGroup>
                            </DropdownMenu.SubContent>
                          </DropdownMenu.Sub>
                        {/if}
                        <DropdownMenu.Item
                          onclick={() => onOpenBoard(item.board_id)}
                        >
                          <LayoutDashboardIcon />
                          {m.smart_view_open_board()}
                        </DropdownMenu.Item>
                      </DropdownMenu.Content>
                    </DropdownMenu.Root>
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
          {#if cursor}
            <div class="flex justify-center">
              <Button
                variant="outline"
                disabled={loading}
                onclick={() => void load(false)}
                >{m.smart_view_load_more()}</Button
              >
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </ScrollArea>
</div>

<ChooseColumnDialog
  bind:open={reopen_open}
  title={m.task_reopen()}
  description={m.task_reopen_description({
    name: reopen_item?.task.title || m.task_untitled(),
  })}
  confirm_label={m.task_reopen_action()}
  columns={reopen_columns}
  onConfirm={reopen}
/>
