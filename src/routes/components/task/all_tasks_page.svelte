<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { getLocale } from "#lib/i18n/index.js";
  import { translateCommandError } from "#lib/command-errors.js";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { toast } from "svelte-sonner";
  import { untrack } from "svelte";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import * as Empty from "#lib/components/ui/empty/index.js";
  import * as InputGroup from "#lib/components/ui/input-group/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import * as Select from "#lib/components/ui/select/index.js";

  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ListFilterPlusIcon from "@lucide/svelte/icons/list-filter-plus";
  import ListTodoIcon from "@lucide/svelte/icons/list-todo";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import SearchIcon from "@lucide/svelte/icons/search";
  import SearchXIcon from "@lucide/svelte/icons/search-x";
  import XIcon from "@lucide/svelte/icons/x";

  import type { AllTaskItem, BoardSummary } from "../../board.svelte";
  import {
    deserialize_column,
    type Column,
    type ColumnSerialized,
  } from "../../type/column.svelte";
  import type { ExplorerConditions, FocusGroup } from "../../type/focus";
  import { priority_label, TASK_PRIORITIES } from "../../type/task.svelte";
  import type { PriorityFilter } from "../../utils/task-explorer";
  import { load_task_page } from "../focus/focus-results";
  import BoardScopeMenu from "./board_scope_menu.svelte";
  import DateFilterPicker from "./date_filter_picker.svelte";
  import PriorityIndicator from "./priority_indicator.svelte";
  import TaskResultList from "./task_result_list.svelte";
  import TaskStatusButtons from "./task_status_buttons.svelte";
  import { task_counts_key, TaskCounts } from "./task-counts.svelte";
  import {
    task_filter_query,
    type ArchiveTimeFilter,
    type DueFilter,
    type SortMode,
    type TaskFilterValues,
    type TaskFilters,
  } from "./task-filters.svelte";

  let {
    filters,
    boards,
    paused = false,
    onOpenTask,
    onEditTask,
    onArchiveTask,
    onOpenBoard,
    onSaveAsFocus,
  }: {
    /** Kept by the caller, so the conditions last while the app is open. */
    filters: TaskFilters;
    boards: BoardSummary[];
    /** Set while a dialog opened from this page may still change tasks. */
    paused?: boolean;
    onOpenTask: (item: AllTaskItem) => unknown;
    onEditTask: (item: AllTaskItem) => unknown;
    onArchiveTask: (item: AllTaskItem) => unknown;
    onOpenBoard: (board_id: number) => void;
    /** Starts a new Focus with the current conditions. */
    onSaveAsFocus?: (conditions: ExplorerConditions) => void;
  } = $props();

  // Most important first, matching the task dialog.
  const priority_filters: PriorityFilter[] = [
    ...[...TASK_PRIORITIES].reverse(),
    "none",
  ];

  let count = $state(0);
  let has_more = $state(false);

  // Typing waits briefly before reading tasks; other changes apply at once
  // after the same short pause.
  const values_key = $derived(JSON.stringify(filters.values));
  // Opening the page shows its current conditions without waiting.
  let applied_key = $state(untrack(() => values_key));
  $effect(() => {
    const key = values_key;
    const timer = setTimeout(() => (applied_key = key), 160);
    return () => clearTimeout(timer);
  });
  const applied = $derived(JSON.parse(applied_key) as TaskFilterValues);
  // Grouping only rearranges the loaded tasks.
  const load_key = $derived(JSON.stringify({ ...applied, group: null }));

  // Counts follow the applied conditions and every saved change, like the
  // list; dialogs opened from the page hold them until they close.
  const counts = new TaskCounts();
  const counts_key = $derived(task_counts_key(applied));
  function load_counts() {
    if (!paused) {
      void counts.load(untrack(() => applied));
    }
  }
  $effect(() => {
    void counts_key;
    void paused;
    untrack(load_counts);
  });
  $effect(() => {
    const unlisten = listen("cardbe:data-changed", load_counts);
    return () => void unlisten.then((stop) => stop());
  });

  const groups: { id: FocusGroup; label: string }[] = $derived([
    { id: "none", label: m.focus_group_none() },
    { id: "board", label: m.focus_group_board() },
    { id: "due", label: m.focus_group_due() },
    { id: "priority", label: m.focus_group_priority() },
  ]);
  const due_options: { id: DueFilter; label: string }[] = $derived([
    { id: "all", label: m.explorer_any_due() },
    { id: "due", label: m.explorer_has_due() },
    { id: "today", label: m.explorer_due_today() },
    { id: "none", label: m.task_due_none() },
    { id: "custom", label: m.explorer_custom_range() },
  ]);
  const archive_options: { id: ArchiveTimeFilter; label: string }[] = $derived([
    { id: "all", label: m.explorer_any_time() },
    { id: "today", label: m.workspace_today() },
    { id: "7d", label: m.explorer_last7() },
    { id: "30d", label: m.explorer_last30() },
    { id: "year", label: m.explorer_year() },
    { id: "custom", label: m.explorer_custom_range() },
  ]);
  const priority_filter_label = $derived(
    new Intl.ListFormat(getLocale(), { type: "unit" }).format(
      priority_filters
        .filter((priority) => filters.priorities.includes(priority))
        .map((priority) =>
          priority === "none"
            ? m.task_priority_none()
            : priority_label(priority),
        ),
    ),
  );
  const invalid_due_range = $derived(
    Boolean(
      filters.due_start &&
      filters.due_end &&
      filters.due_start > filters.due_end,
    ),
  );
  const invalid_archive_range = $derived(
    Boolean(
      filters.archive_start &&
      filters.archive_end &&
      filters.archive_start > filters.archive_end,
    ),
  );

  // Columns of the single selected board, for the column filter.
  const column_board_id = $derived(
    filters.board_ids?.length === 1 ? filters.board_ids[0] : null,
  );
  let columns = $state<Column[]>([]);
  $effect(() => {
    const board_id = column_board_id;
    columns = [];
    if (board_id === null) {
      return;
    }
    let current = true;
    invoke<ColumnSerialized[]>("get_board_columns", { boardId: board_id })
      .then((loaded) => {
        if (current) {
          columns = loaded.map(deserialize_column);
        }
      })
      .catch((error) => {
        if (current) {
          toast.error(translateCommandError(error));
        }
      });
    return () => {
      current = false;
    };
  });
</script>

<svelte:window onfocus={load_counts} />

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  <header class="flex shrink-0 items-center gap-2 border-b px-5 py-3">
    <h2
      class="flex min-w-0 flex-1 items-center gap-2 truncate text-base font-semibold"
    >
      <ListTodoIcon class="size-4 shrink-0 text-primary" />
      {m.all_tasks_title()}
      <span class="text-sm font-normal text-muted-foreground" aria-live="polite"
        >{counts.value
          ? m.task_count({ count: counts.value[applied.status] })
          : has_more
            ? m.explorer_loaded({ count })
            : m.task_count({ count })}</span
      >
    </h2>
    {#if onSaveAsFocus}
      <Button
        variant="outline"
        size="sm"
        title={m.explorer_save_as_focus_hint()}
        onclick={() => onSaveAsFocus(filters.focus_conditions())}
      >
        <ListFilterPlusIcon />{m.explorer_save_as_focus()}
      </Button>
    {/if}
  </header>

  <div class="grid shrink-0 gap-3 border-b px-5 py-3">
    <div class="flex flex-wrap items-end gap-3">
      <InputGroup.Root class="min-w-[14rem] flex-[2]">
        <InputGroup.Input
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
      <div class="min-w-[10rem] flex-1">
        <span id="all-tasks-board-label" class="sr-only"
          >{m.explorer_scope()}</span
        >
        <BoardScopeMenu
          id="all-tasks-board"
          {boards}
          board_ids={filters.board_ids}
          onChange={(board_ids) => filters.set_board_ids(board_ids)}
        />
      </div>
    </div>

    <TaskStatusButtons {filters} counts={counts.value} />

    <div class="flex flex-wrap items-end gap-3">
      <label
        class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
        >{m.explorer_sort_by()}
        <Select.Root
          type="single"
          value={filters.effective_sort}
          onValueChange={(value) =>
            filters.set_sort(
              (value ?? filters.sort_options[0]?.id ?? "due") as SortMode,
            )}
        >
          <Select.Trigger
            class="h-9 w-full min-w-0"
            aria-label={m.explorer_sort_tasks()}
          >
            {filters.sort_options.find(
              (option) => option.id === filters.effective_sort,
            )?.label ?? filters.sort_options[0]?.label}
          </Select.Trigger>
          <Select.Content>
            {#each filters.sort_options as option (option.id)}
              <Select.Item value={option.id}>{option.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </label>
      <label
        class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
        >{m.focus_group()}
        <Select.Root
          type="single"
          value={filters.group}
          onValueChange={(value) =>
            (filters.group = (value ?? "none") as FocusGroup)}
        >
          <Select.Trigger class="h-9 w-full min-w-0">
            {groups.find((option) => option.id === filters.group)?.label}
          </Select.Trigger>
          <Select.Content>
            {#each groups as option (option.id)}
              <Select.Item value={option.id}>{option.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </label>
      <label
        class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
        >{m.task_due_date()}
        <Select.Root
          type="single"
          value={filters.due}
          onValueChange={(value) =>
            (filters.due = (value ?? "all") as DueFilter)}
        >
          <Select.Trigger class="h-9 w-full min-w-0">
            {due_options.find((option) => option.id === filters.due)?.label}
          </Select.Trigger>
          <Select.Content>
            {#each due_options as option (option.id)}
              <Select.Item value={option.id}>{option.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </label>
      <div
        class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
      >
        <span id="all-tasks-priority-label">{m.task_priority()}</span>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Button
                {...props}
                variant="outline"
                class="h-9 w-full min-w-0 justify-between font-normal"
                aria-labelledby="all-tasks-priority-label all-tasks-priority-selection"
              >
                <span id="all-tasks-priority-selection" class="truncate"
                  >{filters.priorities.length > 0
                    ? priority_filter_label
                    : m.explorer_any_priority()}</span
                >
                <ChevronDownIcon class="size-4" />
              </Button>
            {/snippet}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="start">
            {#each priority_filters as priority (priority)}
              <DropdownMenu.CheckboxItem
                checked={filters.priorities.includes(priority)}
                closeOnSelect={false}
                onCheckedChange={(checked) => {
                  filters.priorities = checked
                    ? [...filters.priorities, priority]
                    : filters.priorities.filter((item) => item !== priority);
                }}
              >
                {#if priority === "none"}
                  {m.task_priority_none()}
                {:else}
                  <PriorityIndicator {priority} />
                {/if}
              </DropdownMenu.CheckboxItem>
            {/each}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </div>
      {#if column_board_id !== null && filters.status !== "archived"}
        <label
          class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
        >
          {m.explorer_column()}
          <Select.Root
            type="single"
            value={filters.column}
            onValueChange={(value) => (filters.column = value ?? "all")}
          >
            <Select.Trigger class="h-9 w-full min-w-0">
              {columns.find((column) => column.id === filters.column)?.name ??
                m.explorer_all_columns()}
            </Select.Trigger>
            <Select.Content>
              <Select.Item value="all">{m.explorer_all_columns()}</Select.Item>
              {#each columns as column (column.id)}
                <Select.Item value={column.id}>{column.name}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </label>
      {/if}
      {#if filters.status === "archived" || filters.status === "all"}
        <label
          class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
        >
          {m.explorer_archive_time()}
          <Select.Root
            type="single"
            value={filters.archive_time}
            onValueChange={(value) =>
              (filters.archive_time = (value ?? "all") as ArchiveTimeFilter)}
          >
            <Select.Trigger class="h-9 w-full min-w-0">
              {archive_options.find(
                (option) => option.id === filters.archive_time,
              )?.label}
            </Select.Trigger>
            <Select.Content>
              {#each archive_options as option (option.id)}
                <Select.Item value={option.id}>{option.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </label>
      {/if}
      {#if filters.active_filter_count > 0}
        <Button
          variant="ghost"
          size="sm"
          class="h-9"
          title={m.explorer_clear_all_hint()}
          onclick={() => filters.reset_filters()}
        >
          <RotateCcwIcon />{m.explorer_clear_all()}
        </Button>
      {/if}
    </div>

    {#if filters.due === "custom"}
      <div class="grid max-w-xl grid-cols-2 gap-3">
        <DateFilterPicker
          label={m.explorer_due_from()}
          bind:value={filters.due_start}
          max={filters.due_end || undefined}
          invalid={invalid_due_range}
          describedby="all-tasks-due-range-help"
        />
        <DateFilterPicker
          label={m.explorer_due_through()}
          bind:value={filters.due_end}
          min={filters.due_start || undefined}
          invalid={invalid_due_range}
          describedby="all-tasks-due-range-help"
        />
      </div>
      <p
        id="all-tasks-due-range-help"
        class={invalid_due_range
          ? "text-xs text-destructive"
          : "text-xs text-muted-foreground"}
        role={invalid_due_range ? "alert" : undefined}
      >
        {invalid_due_range ? m.explorer_range_invalid() : m.explorer_due_help()}
      </p>
    {/if}
    {#if filters.archive_time === "custom"}
      <div class="grid max-w-xl grid-cols-2 gap-3">
        <DateFilterPicker
          label={m.ui_start_date()}
          bind:value={filters.archive_start}
          max={filters.archive_end || undefined}
          invalid={invalid_archive_range}
          describedby="all-tasks-archive-range-help"
        />
        <DateFilterPicker
          label={m.explorer_end()}
          bind:value={filters.archive_end}
          min={filters.archive_start || undefined}
          invalid={invalid_archive_range}
          describedby="all-tasks-archive-range-help"
        />
      </div>
      <p
        id="all-tasks-archive-range-help"
        class={invalid_archive_range
          ? "text-xs text-destructive"
          : "text-xs text-muted-foreground"}
        role={invalid_archive_range ? "alert" : undefined}
      >
        {invalid_archive_range
          ? m.explorer_range_invalid()
          : m.explorer_archive_help()}
      </p>
    {/if}
  </div>

  <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="vertical">
    <div class="mx-auto w-full max-w-5xl p-5">
      {#if applied.board_ids?.length === 0}
        <Empty.Root>
          <Empty.Header>
            <Empty.Media variant="icon"><ListTodoIcon /></Empty.Media>
            <Empty.Title>{m.explorer_no_boards()}</Empty.Title>
            <Empty.Description
              >{m.explorer_select_board_hint()}</Empty.Description
            >
          </Empty.Header>
        </Empty.Root>
      {:else}
        <TaskResultList
          bind:count
          bind:has_more
          {load_key}
          load={(now, cursor, limit) =>
            load_task_page(task_filter_query(applied, now), cursor, limit)}
          group={filters.group}
          label={m.all_tasks_title()}
          {boards}
          {paused}
          highlight={applied.search_text}
          {onOpenTask}
          {onEditTask}
          {onArchiveTask}
          {onOpenBoard}
        >
          {#snippet empty()}
            <Empty.Root>
              <Empty.Header>
                <Empty.Media variant="icon"><SearchXIcon /></Empty.Media>
                <Empty.Title>{m.explorer_no_matching()}</Empty.Title>
                <Empty.Description>{m.explorer_search_hint()}</Empty.Description
                >
              </Empty.Header>
              {#if filters.searching || filters.active_filter_count > 0}
                <Empty.Content>
                  <Button
                    variant="outline"
                    title={m.explorer_clear_search_hint()}
                    onclick={() => filters.clear()}
                    >{m.explorer_clear_search_filters()}</Button
                  >
                </Empty.Content>
              {/if}
            </Empty.Root>
          {/snippet}
        </TaskResultList>
      {/if}
    </div>
  </ScrollArea>
</div>
