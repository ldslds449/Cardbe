<script lang="ts">
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import CalendarClockIcon from "@lucide/svelte/icons/calendar-clock";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import Columns3Icon from "@lucide/svelte/icons/columns-3";
  import ListTodoIcon from "@lucide/svelte/icons/list-todo";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import SearchIcon from "@lucide/svelte/icons/search";
  import SlidersHorizontalIcon from "@lucide/svelte/icons/sliders-horizontal";
  import ArrowUpDownIcon from "@lucide/svelte/icons/arrow-up-down";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import Undo2Icon from "@lucide/svelte/icons/undo-2";
  import XIcon from "@lucide/svelte/icons/x";

  import * as Empty from "$lib/components/ui/empty/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as Sheet from "$lib/components/ui/sheet/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import DateFilterPicker from "./date_filter_picker.svelte";
  import { cn } from "$lib/utils.js";

  import type { AllTaskItem, BoardSummary } from "../../board.svelte";
  import type { Column } from "../../type/column.svelte";
  import type { Task, TaskSummary } from "../../type/task.svelte";
  import {
    is_overdue,
    date_range_bounds,
    type TaskExplorerQuery,
  } from "../../utils/task-explorer";
  import { get_column_id } from "../../type/column.svelte";
  import { display_task_color } from "../../utils/task-color";
  import UnarchiveTaskDialog from "../archive/unarchive_task_dialog.svelte";
  import VirtualList from "../virtual_list.svelte";

  type SmartView = "all" | "active" | "overdue" | "recurring" | "archived";
  type DueFilter = "all" | "due" | "none" | "today" | "custom";
  type SortMode = "due" | "title" | "column" | "archived";
  type ArchiveTimeFilter = "all" | "today" | "7d" | "30d" | "year" | "custom";
  type ExplorerEntry = AllTaskItem;

  let {
    open = $bindable(false),
    active_board_id,
    boards,
    columns,
    all_task_items,
    all_task_items_loading = false,
    all_task_items_has_more = false,
    all_task_items_error = false,
    read_only = false,
    onSwitchBoard,
    onLoadMoreAllTasks,
    onSearchAllTasks,
    onGetArchiveDetail,
    onGetExpiredDetail,
    onUnarchive,
    onViewTask,
    onViewArchivedTask,
    onEditTask,
    onArchiveTask,
  }: {
    open: boolean;
    active_board_id: number | null;
    boards: BoardSummary[];
    columns: Column[];
    all_task_items: AllTaskItem[];
    all_task_items_loading?: boolean;
    all_task_items_has_more?: boolean;
    all_task_items_error?: boolean;
    read_only?: boolean;
    onSwitchBoard: (id: number) => void | Promise<void>;
    onLoadMoreAllTasks: () => void;
    onSearchAllTasks: (filter: TaskExplorerQuery) => void;
    onGetArchiveDetail: (task_id: string) => Promise<Task | null>;
    onGetExpiredDetail: (task_id: string) => Promise<Task | null>;
    onUnarchive: (column_id: string, task_id: string) => void;
    onViewTask: (task: Task) => void;
    onViewArchivedTask: (task: Task) => void;
    onEditTask: (task: Task) => void;
    onArchiveTask: (task: Task) => void;
  } = $props();

  let smart_view = $state<SmartView>("active");
  let board_scope = $state<number[] | null>(null);
  let controls_open = $state(false);
  const global_scope = $derived(
    board_scope?.length !== 1 || board_scope[0] !== active_board_id,
  );
  const scope_name = $derived(
    board_scope === null
      ? "All boards"
      : board_scope.length === 0
        ? "No boards selected"
        : board_scope.length === 1
          ? (boards.find((item) => item.id === board_scope?.[0])?.name ??
            "1 board")
          : `${board_scope.length} boards selected`,
  );
  let search_text = $state("");
  let column_filter = $state("all");
  let due_filter = $state<DueFilter>("all");
  let sort_mode = $state<SortMode>("due");
  let archive_time_filter = $state<ArchiveTimeFilter>("all");
  let archive_start = $state("");
  let archive_end = $state("");
  let due_start = $state("");
  let due_end = $state("");
  const invalid_due_range = $derived(
    Boolean(due_start && due_end && due_start > due_end),
  );
  const invalid_archive_range = $derived(
    Boolean(archive_start && archive_end && archive_start > archive_end),
  );
  const active_filter_count = $derived(
    Number(column_filter !== "all") +
      Number(due_filter !== "all") +
      Number(archive_time_filter !== "all"),
  );
  let now = $state(new Date());
  let detail_loading_id = $state<string | null>(null);
  let unarchive_open = $state(false);
  let unarchive_target = $state<TaskSummary | null>(null);
  let requested_query = $state("");

  const smart_views: {
    id: SmartView;
    label: string;
  }[] = [
    { id: "active", label: "Active" },
    { id: "overdue", label: "Overdue" },
    { id: "recurring", label: "Recurring" },
    { id: "archived", label: "Archived" },
    { id: "all", label: "All statuses" },
  ];

  const column_options = $derived([
    { id: "all", label: "All columns" },
    ...columns.map((column) => ({ id: column.id, label: column.name })),
  ]);
  const sort_options = $derived.by(() =>
    smart_view === "archived"
      ? [
          { id: "archived" as const, label: "Archived at" },
          { id: "title" as const, label: "Title" },
        ]
      : smart_view === "recurring"
        ? [
            { id: "title" as const, label: "Title" },
            { id: "column" as const, label: "Column" },
          ]
        : [
            { id: "due" as const, label: "Due date" },
            { id: "title" as const, label: "Title" },
            { id: "column" as const, label: "Column" },
          ],
  );

  const query = $derived.by((): TaskExplorerQuery => {
    const today = new Date(now);
    today.setHours(0, 0, 0, 0);
    const tomorrow = new Date(today);
    tomorrow.setDate(tomorrow.getDate() + 1);
    const [due_from, due_until] = date_range_bounds(due_start, due_end);
    let [archive_from, archive_until] = date_range_bounds(
      archive_start,
      archive_end,
    );
    if (archive_time_filter !== "custom") {
      const start = new Date(today);
      if (archive_time_filter === "7d") {
        start.setDate(start.getDate() - 6);
      }
      if (archive_time_filter === "30d") {
        start.setDate(start.getDate() - 29);
      }
      if (archive_time_filter === "year") {
        start.setMonth(0, 1);
      }
      archive_from = archive_time_filter === "all" ? null : start.getTime();
      archive_until = archive_time_filter === "all" ? null : tomorrow.getTime();
    }
    return {
      query: search_text.trim(),
      board_ids: board_scope,
      column_id:
        !global_scope && column_filter !== "all"
          ? get_column_id(column_filter)
          : null,
      status: smart_view,
      sort: sort_mode,
      due:
        due_filter === "today" || due_filter === "custom" ? "due" : due_filter,
      due_start:
        due_filter === "today"
          ? today.getTime()
          : due_filter === "custom"
            ? due_from
            : null,
      due_end:
        due_filter === "today"
          ? tomorrow.getTime()
          : due_filter === "custom"
            ? due_until
            : null,
      archive_start: archive_from,
      archive_end: archive_until,
      now: now.getTime(),
    };
  });
  const query_key = $derived(JSON.stringify(query));
  const pending = $derived(requested_query !== query_key);
  const entries: ExplorerEntry[] = $derived(pending ? [] : all_task_items);
  const loading = $derived(pending || all_task_items_loading);
  const has_more = $derived(!pending && all_task_items_has_more);
  const row_height = 72;
  const has_conditions = $derived(
    Boolean(search_text.trim() || active_filter_count),
  );
  const empty_title = $derived(
    board_scope?.length === 0
      ? "No boards selected"
      : has_conditions
        ? "No matching tasks"
        : smart_view === "all"
          ? "No tasks yet"
          : `No ${smart_view} tasks`,
  );

  $effect(() => {
    if (!open) {
      return;
    }
    now = new Date();
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

  $effect(() => {
    if (global_scope) {
      column_filter = "all";
    }
  });

  function set_view(view: SmartView) {
    smart_view = view;
    if (view === "archived") {
      column_filter = "all";
    }
    if (view !== "archived" && view !== "all") {
      archive_time_filter = "all";
      archive_start = "";
      archive_end = "";
    }
    if (!sort_options.some((option) => option.id === sort_mode)) {
      sort_mode = sort_options[0].id;
    }
  }

  function format_due(task: Task | TaskSummary): string {
    if (!task.due_time) {
      return "No due date";
    }
    return task.due_time.toLocaleString("en", {
      month: "short",
      day: "numeric",
      hour:
        task.due_time.getHours() || task.due_time.getMinutes()
          ? "2-digit"
          : undefined,
      minute:
        task.due_time.getHours() || task.due_time.getMinutes()
          ? "2-digit"
          : undefined,
      year:
        task.due_time.getFullYear() === now.getFullYear()
          ? undefined
          : "numeric",
    });
  }

  function format_archive_time(archived_at: Date): string {
    return `Archived ${archived_at.toLocaleDateString("en", {
      month: "short",
      day: "numeric",
      year:
        archived_at.getFullYear() === now.getFullYear() ? undefined : "numeric",
    })}`;
  }

  function entry_column(entry: ExplorerEntry): string {
    if (entry.archived_at) {
      return entry.board_name;
    }
    return `${entry.board_name} / ${entry.column_name ?? "No column"}`;
  }

  function clear_filters() {
    search_text = "";
    reset_filters();
  }

  function reset_filters() {
    column_filter = "all";
    due_filter = "all";
    due_start = "";
    due_end = "";
    archive_time_filter = "all";
    archive_start = "";
    archive_end = "";
  }

  function select_board(board_id: number, checked: boolean) {
    const selected = board_scope ?? boards.map((board) => board.id);
    board_scope = selected.filter((id) => id !== board_id);
    if (checked) {
      board_scope = [...board_scope, board_id].sort((a, b) => a - b);
    }
    column_filter = "all";
  }

  function show_unarchive(task: TaskSummary) {
    if (read_only) {
      return;
    }
    unarchive_target = task;
    unarchive_open = true;
  }

  function entry_read_only(entry: ExplorerEntry): boolean {
    return (
      boards.find((board) => board.id === entry.board_id)?.shared_role ===
      "viewer"
    );
  }

  function entry_archived(entry: ExplorerEntry): boolean {
    return Boolean(entry.archived_at);
  }

  async function get_entry_task(entry: ExplorerEntry): Promise<Task | null> {
    if (detail_loading_id) {
      return null;
    }
    detail_loading_id = entry.task.id;
    try {
      await onSwitchBoard(entry.board_id);
      if (active_board_id !== entry.board_id) {
        return null;
      }
      return await (entry_archived(entry)
        ? onGetArchiveDetail(entry.task.id)
        : onGetExpiredDetail(entry.task.id));
    } finally {
      detail_loading_id = null;
    }
  }

  async function load_detail(entry: ExplorerEntry, edit = false) {
    if (edit && entry_read_only(entry)) {
      return;
    }
    const task = await get_entry_task(entry);
    if (task) {
      if (edit) {
        if (!read_only) {
          onEditTask(task);
        }
      } else if (entry_archived(entry)) {
        onViewArchivedTask(task);
      } else {
        onViewTask(task);
      }
    }
  }

  async function archive_entry(entry: ExplorerEntry) {
    if (entry_read_only(entry) || entry_archived(entry)) {
      return;
    }
    const task = await get_entry_task(entry);
    if (task && !read_only) {
      onArchiveTask(task);
    }
  }

  async function unarchive_entry(entry: ExplorerEntry) {
    if (entry_read_only(entry) || !entry_archived(entry)) {
      return;
    }
    const task = await get_entry_task(entry);
    if (task && !read_only) {
      show_unarchive(task);
    }
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
  >
    <Sheet.Header class="border-b px-6 pb-3 pt-4">
      <div class="flex items-start justify-between gap-4 pr-8">
        <div class="min-w-0">
          <Sheet.Title class="flex items-center gap-2">
            <ListTodoIcon class="size-5 text-primary" />
            Task Explorer
          </Sheet.Title>
          <Sheet.Description class="sr-only"
            >Search and manage tasks across boards.</Sheet.Description
          >
        </div>
      </div>
    </Sheet.Header>

    <div
      class="grid max-h-[65%] shrink-0 gap-3 overflow-y-auto border-b px-6 py-3"
    >
      <div class="flex items-center justify-between gap-3">
        <div aria-live="polite">
          <span class="text-base font-semibold"
            >{entries.length} {has_more ? "tasks loaded" : "tasks"}</span
          >
        </div>
        <Button
          variant="outline"
          size="sm"
          aria-expanded={controls_open}
          aria-controls="task-explorer-controls"
          onclick={() => (controls_open = !controls_open)}
        >
          <SlidersHorizontalIcon class="size-4" />{controls_open
            ? "Hide filters"
            : "Show filters"}
          {#if active_filter_count}<Badge variant="secondary"
              >{active_filter_count}</Badge
            >{/if}
          <ChevronDownIcon
            class={cn(
              "size-4 transition-transform",
              controls_open && "rotate-180",
            )}
          />
        </Button>
      </div>
      <div class="flex flex-wrap items-end gap-3">
        <div
          class="grid min-w-[10rem] flex-[1.2] gap-1 text-xs font-medium text-muted-foreground"
        >
          <span id="task-explorer-board-label">Board scope</span>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="outline"
                  class="w-full min-w-0 justify-between"
                  aria-labelledby="task-explorer-board-label task-explorer-board-selection"
                >
                  <span id="task-explorer-board-selection" class="truncate"
                    >{scope_name}</span
                  >
                  <ChevronDownIcon class="size-4" />
                </Button>
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content
              class="max-h-80 overflow-y-auto"
              align="start"
            >
              <DropdownMenu.CheckboxItem
                checked={board_scope === null ||
                  (boards.length > 0 && board_scope.length === boards.length)}
                indeterminate={board_scope !== null &&
                  board_scope.length > 0 &&
                  board_scope.length < boards.length}
                closeOnSelect={false}
                onCheckedChange={(checked) => {
                  board_scope = checked ? null : [];
                  column_filter = "all";
                }}>All boards</DropdownMenu.CheckboxItem
              >
              <DropdownMenu.Separator />
              {#each boards as board (board.id)}
                <DropdownMenu.CheckboxItem
                  checked={board_scope === null ||
                    board_scope.includes(board.id)}
                  closeOnSelect={false}
                  onCheckedChange={(checked) => select_board(board.id, checked)}
                  >{board.name}</DropdownMenu.CheckboxItem
                >
              {/each}
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
        <label
          class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
          >Sort by
          <Select.Root
            type="single"
            value={sort_mode}
            onValueChange={(value) =>
              (sort_mode = (value ?? sort_options[0]?.id ?? "due") as SortMode)}
          >
            <Select.Trigger
              class="h-9 w-full min-w-0 gap-2"
              aria-label="Sort tasks"
            >
              <ArrowUpDownIcon class="size-4" />
              {sort_options.find((option) => option.id === sort_mode)?.label ??
                sort_options[0]?.label}
            </Select.Trigger>
            <Select.Content>
              {#each sort_options as option (option.id)}<Select.Item
                  value={option.id}>{option.label}</Select.Item
                >{/each}
            </Select.Content>
          </Select.Root>
        </label>
      </div>
      <InputGroup.Root>
        <InputGroup.Input
          placeholder={board_scope === null
            ? "Search all boards"
            : `Search tasks in ${scope_name}`}
          aria-label={board_scope === null
            ? "Search all boards"
            : `Search tasks in ${scope_name}`}
          bind:value={search_text}
        />
        <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
        {#if search_text}
          <InputGroup.Addon align="inline-end">
            <InputGroup.Button
              aria-label="Clear task search"
              size="icon-xs"
              onclick={() => (search_text = "")}
            >
              <XIcon />
            </InputGroup.Button>
          </InputGroup.Addon>
        {/if}
      </InputGroup.Root>

      <div class="flex flex-wrap gap-2" role="group" aria-label="Task status">
        {#each smart_views as view (view.id)}
          <Button
            variant={smart_view === view.id ? "default" : "outline"}
            size="sm"
            class="shrink-0"
            aria-pressed={smart_view === view.id}
            onclick={() => set_view(view.id)}>{view.label}</Button
          >
        {/each}
      </div>
      {#if active_filter_count}
        <div
          class="flex flex-wrap items-center gap-2"
          aria-label="Applied filters"
        >
          {#if column_filter !== "all"}
            <Button
              variant="secondary"
              size="xs"
              title="Clear column filter"
              onclick={() => (column_filter = "all")}
            >
              Column: {column_options.find(
                (column) => column.id === column_filter,
              )?.label}
              <XIcon class="size-3" />
            </Button>
          {/if}
          {#if due_filter !== "all"}
            <Button
              variant="secondary"
              size="xs"
              title="Clear due date filter"
              onclick={() => {
                due_filter = "all";
                due_start = "";
                due_end = "";
              }}
              >{due_filter === "due"
                ? "Has due date"
                : due_filter === "today"
                  ? "Due today"
                  : due_filter === "custom"
                    ? `Due: ${due_start || "Any start"} – ${due_end || "Any end"}`
                    : "No due date"}
              <XIcon class="size-3" />
            </Button>
          {/if}
          {#if archive_time_filter !== "all"}
            <Button
              variant="secondary"
              size="xs"
              title="Clear archive time filter"
              onclick={() => {
                archive_time_filter = "all";
                archive_start = "";
                archive_end = "";
              }}
              >Archived: {archive_time_filter === "today"
                ? "Today"
                : archive_time_filter === "7d"
                  ? "Last 7 days"
                  : archive_time_filter === "30d"
                    ? "Last 30 days"
                    : archive_time_filter === "custom"
                      ? `${archive_start || "Any start"} – ${archive_end || "Any end"}`
                      : "This year"}
              <XIcon class="size-3" />
            </Button>
          {/if}
          <Button
            variant="ghost"
            size="xs"
            title="Clear column, due date, and archive time filters"
            onclick={reset_filters}
            ><RotateCcwIcon class="size-3" />Clear all filters</Button
          >
        </div>
      {/if}

      <div id="task-explorer-controls" hidden={!controls_open}>
        {#if controls_open}
          <div class="grid gap-3">
            <div class="flex flex-wrap items-end gap-2">
              {#if !global_scope && smart_view !== "archived"}
                <label
                  class="grid min-w-[9rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
                >
                  Column
                  <Select.Root
                    type="single"
                    value={column_filter}
                    onValueChange={(value) => (column_filter = value ?? "all")}
                  >
                    <Select.Trigger class="h-9 w-full min-w-0">
                      {column_options.find(
                        (column) => column.id === column_filter,
                      )?.label ?? "All columns"}
                    </Select.Trigger>
                    <Select.Content>
                      {#each column_options as column (column.id)}
                        <Select.Item value={column.id}
                          >{column.label}</Select.Item
                        >
                      {/each}
                    </Select.Content>
                  </Select.Root>
                </label>
              {/if}
              <label
                class="grid min-w-[8rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
              >
                Due
                <Select.Root
                  type="single"
                  value={due_filter}
                  onValueChange={(value) =>
                    (due_filter = (value ?? "all") as DueFilter)}
                >
                  <Select.Trigger class="h-9 w-full min-w-0">
                    {due_filter === "all"
                      ? "Any due date"
                      : due_filter === "due"
                        ? "Has due date"
                        : due_filter === "today"
                          ? "Due today"
                          : due_filter === "custom"
                            ? "Custom range"
                            : "No due date"}
                  </Select.Trigger>
                  <Select.Content>
                    <Select.Item value="all">Any due date</Select.Item>
                    <Select.Item value="due">Has due date</Select.Item>
                    <Select.Item value="today">Due today</Select.Item>
                    <Select.Item value="none">No due date</Select.Item>
                    <Select.Item value="custom">Custom range</Select.Item>
                  </Select.Content>
                </Select.Root>
              </label>
            </div>
            {#if due_filter === "custom"}
              <div class="grid grid-cols-2 gap-3">
                <DateFilterPicker
                  label="Due from"
                  bind:value={due_start}
                  max={due_end || undefined}
                  invalid={invalid_due_range}
                  describedby="due-range-help"
                />
                <DateFilterPicker
                  label="Due through"
                  bind:value={due_end}
                  min={due_start || undefined}
                  invalid={invalid_due_range}
                  describedby="due-range-help"
                />
              </div>
              <p
                id="due-range-help"
                class={invalid_due_range
                  ? "text-xs text-destructive"
                  : "text-xs text-muted-foreground"}
                role={invalid_due_range ? "alert" : undefined}
              >
                {invalid_due_range
                  ? "End date must be on or after start date."
                  : "Includes both dates. Tasks without a due date are excluded. Leave either date empty for an open-ended range."}
              </p>
            {/if}
            {#if smart_view === "archived" || smart_view === "all"}
              <div class="flex flex-wrap items-end gap-2">
                <label
                  class="grid min-w-[10rem] flex-1 gap-1 text-xs font-medium text-muted-foreground"
                >
                  Archive time
                  <Select.Root
                    type="single"
                    value={archive_time_filter}
                    onValueChange={(value) =>
                      (archive_time_filter = (value ??
                        "all") as ArchiveTimeFilter)}
                  >
                    <Select.Trigger class="h-9 w-full min-w-0">
                      {archive_time_filter === "all"
                        ? "Any time"
                        : archive_time_filter === "today"
                          ? "Today"
                          : archive_time_filter === "7d"
                            ? "Last 7 days"
                            : archive_time_filter === "30d"
                              ? "Last 30 days"
                              : archive_time_filter === "custom"
                                ? "Custom range"
                                : "This year"}
                    </Select.Trigger>
                    <Select.Content>
                      <Select.Item value="all">Any time</Select.Item>
                      <Select.Item value="today">Today</Select.Item>
                      <Select.Item value="7d">Last 7 days</Select.Item>
                      <Select.Item value="30d">Last 30 days</Select.Item>
                      <Select.Item value="year">This year</Select.Item>
                      <Select.Item value="custom">Custom range</Select.Item>
                    </Select.Content>
                  </Select.Root>
                </label>
              </div>
              {#if archive_time_filter === "custom"}
                <div class="grid grid-cols-2 gap-3">
                  <DateFilterPicker
                    label="Start date"
                    bind:value={archive_start}
                    max={archive_end || undefined}
                    invalid={invalid_archive_range}
                    describedby="archive-range-help"
                  />
                  <DateFilterPicker
                    label="End date"
                    bind:value={archive_end}
                    min={archive_start || undefined}
                    invalid={invalid_archive_range}
                    describedby="archive-range-help"
                  />
                </div>
                <p
                  id="archive-range-help"
                  class={invalid_archive_range
                    ? "text-xs text-destructive"
                    : "text-xs text-muted-foreground"}
                  role={invalid_archive_range ? "alert" : undefined}
                >
                  {invalid_archive_range
                    ? "End date must be on or after start date."
                    : "Includes both dates. Leave either date empty for an open-ended range."}
                </p>
              {/if}
            {/if}
          </div>
        {/if}
      </div>
    </div>

    <div class="flex min-h-0 flex-1 flex-col px-4 py-3">
      {#if all_task_items_error && !loading && entries.length === 0}
        <Empty.Root class="h-full border-none">
          <Empty.Header
            ><Empty.Title>Couldn't load tasks</Empty.Title>
            <Empty.Description>Try loading the list again.</Empty.Description
            ></Empty.Header
          >
          <Button variant="outline" onclick={() => onSearchAllTasks(query)}
            >Retry</Button
          >
        </Empty.Root>
      {:else if loading && entries.length === 0}
        <div
          class="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground"
        >
          <Spinner />
          Loading {smart_view} tasks...
        </div>
      {:else if entries.length === 0 && !has_more}
        <Empty.Root class="h-full border-none py-12">
          <Empty.Header>
            <Empty.Media variant="icon"><ListTodoIcon /></Empty.Media>
            <Empty.Title>{empty_title}</Empty.Title>
            <Empty.Description
              >{board_scope?.length === 0
                ? "Select at least one board to view tasks."
                : has_conditions
                  ? "Try another search or clear the filters."
                  : `There are no ${smart_view === "all" ? "" : smart_view + " "}tasks in ${scope_name}.`}</Empty.Description
            >
            {#if has_conditions}
              <Button
                variant="outline"
                title="Clear the search text and all applied filters"
                onclick={clear_filters}>Clear search and filters</Button
              >
            {/if}
          </Empty.Header>
        </Empty.Root>
      {:else}
        <VirtualList
          items={entries}
          rowHeight={row_height}
          class="min-h-0 flex-1"
          hasMore={has_more && !all_task_items_error}
          {loading}
          resetKey={query_key}
          onLoadMore={onLoadMoreAllTasks}
        >
          {#snippet children(entry: ExplorerEntry, _index: number)}
            <ContextMenu.Root>
              <ContextMenu.Trigger class="block h-full">
                <article
                  class="group flex h-full items-center gap-2 border-b border-border/60 px-3 py-2 transition-colors hover:bg-muted/50"
                >
                  <button
                    type="button"
                    class="min-w-0 flex-1 self-stretch text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                    aria-label={`View ${entry.task.title || "untitled task"}`}
                    title={entry.board_id !== active_board_id
                      ? `Opens this task in ${entry.board_name}`
                      : undefined}
                    onclick={() => void load_detail(entry)}
                  >
                    <div class="flex min-w-0 items-start gap-3">
                      <span
                        class="mt-1 size-2 shrink-0 rounded-full"
                        style={`background-color: ${display_task_color(entry.task.color) || "var(--muted-foreground)"}`}
                      ></span>
                      <span class="min-w-0 flex-1">
                        <span
                          class="block truncate text-sm font-medium leading-5 text-foreground"
                        >
                          {entry.task.title || "Untitled task"}
                        </span>
                        <span
                          class="mt-1 flex min-w-0 items-center gap-3 text-xs text-muted-foreground"
                        >
                          <span
                            class="inline-flex min-w-0 flex-1 items-center gap-1"
                          >
                            {#if !entry.archived_at}<Columns3Icon
                                class="size-3.5"
                              />{/if}
                            <span class="truncate">{entry_column(entry)}</span>
                          </span>
                          <span
                            class={cn(
                              "inline-flex shrink-0 items-center gap-1",
                              is_overdue(entry.task, now, entry.archived_at) &&
                                "font-medium text-destructive",
                            )}
                          >
                            {#if entry.archived_at}
                              <ArchiveIcon class="size-3.5" />
                              {format_archive_time(entry.archived_at)}
                            {:else if entry.task.due_time}
                              {#if is_overdue(entry.task, now, entry.archived_at)}
                                <CircleAlertIcon class="size-3.5" />
                              {:else}
                                <CalendarClockIcon class="size-3.5" />
                              {/if}
                              {format_due(entry.task)}
                            {/if}
                          </span>
                          {#if entry_read_only(entry)}
                            <Badge
                              variant="outline"
                              class="shrink-0 py-0 text-[11px]">Read only</Badge
                            >
                          {/if}
                          {#if smart_view !== "recurring" && entry.task.recurrence && !entry.archived_at}
                            <Badge
                              variant="outline"
                              class="gap-1 py-0 text-[11px]"
                            >
                              <Repeat2Icon />
                              Recurring
                            </Badge>
                          {/if}
                        </span>
                      </span>
                    </div>
                  </button>
                  {#if detail_loading_id === entry.task.id}
                    <span class="detail-loading-indicator">
                      <Spinner class="size-4" />
                    </span>
                  {/if}
                  <DropdownMenu.Root>
                    <DropdownMenu.Trigger>
                      {#snippet child({ props })}
                        <Button
                          {...props}
                          variant="ghost"
                          size="icon-sm"
                          aria-label={`Actions for ${entry.task.title || "untitled task"}`}
                          title="Task actions"
                          ><EllipsisIcon class="size-4" /></Button
                        >
                      {/snippet}
                    </DropdownMenu.Trigger>
                    <DropdownMenu.Content align="end">
                      <DropdownMenu.Item
                        disabled={Boolean(detail_loading_id)}
                        onclick={() => void load_detail(entry)}
                        ><ListTodoIcon class="size-4" />View task</DropdownMenu.Item
                      >
                      {#if !entry_read_only(entry)}
                        <DropdownMenu.Separator />
                        {#if entry_archived(entry)}
                          <DropdownMenu.Item
                            disabled={Boolean(detail_loading_id)}
                            onclick={() => void unarchive_entry(entry)}
                            ><Undo2Icon class="size-4" />Unarchive task</DropdownMenu.Item
                          >
                        {:else}
                          <DropdownMenu.Item
                            disabled={Boolean(detail_loading_id)}
                            onclick={() => void load_detail(entry, true)}
                            ><PencilIcon class="size-4" />Edit task</DropdownMenu.Item
                          >
                          <DropdownMenu.Item
                            disabled={Boolean(detail_loading_id)}
                            onclick={() => void archive_entry(entry)}
                            ><ArchiveIcon class="size-4" />Archive task</DropdownMenu.Item
                          >
                        {/if}
                      {/if}
                    </DropdownMenu.Content>
                  </DropdownMenu.Root>
                </article>
              </ContextMenu.Trigger>
              <ContextMenu.Content>
                <ContextMenu.Item
                  disabled={Boolean(detail_loading_id)}
                  onclick={() => void load_detail(entry)}
                  ><ListTodoIcon class="size-4" />View task</ContextMenu.Item
                >
                {#if !entry_read_only(entry)}
                  <ContextMenu.Separator />
                  {#if entry_archived(entry)}
                    <ContextMenu.Item
                      disabled={Boolean(detail_loading_id)}
                      onclick={() => void unarchive_entry(entry)}
                      ><Undo2Icon class="size-4" />Unarchive task</ContextMenu.Item
                    >
                  {:else}
                    <ContextMenu.Item
                      disabled={Boolean(detail_loading_id)}
                      onclick={() => void load_detail(entry, true)}
                      ><PencilIcon class="size-4" />Edit task</ContextMenu.Item
                    >
                    <ContextMenu.Item
                      disabled={Boolean(detail_loading_id)}
                      onclick={() => void archive_entry(entry)}
                      ><ArchiveIcon class="size-4" />Archive task</ContextMenu.Item
                    >
                  {/if}
                {/if}
              </ContextMenu.Content>
            </ContextMenu.Root>
          {/snippet}
        </VirtualList>
        {#if loading}
          <div
            class="flex items-center justify-center gap-2 py-2 text-xs text-muted-foreground"
          >
            <Spinner />Loading more...
          </div>
        {/if}
        {#if all_task_items_error && !loading}
          <Button
            variant="outline"
            class="shrink-0"
            onclick={onLoadMoreAllTasks}>Retry loading more tasks</Button
          >
        {/if}
      {/if}
    </div>
  </Sheet.Content>
</Sheet.Root>

<UnarchiveTaskDialog
  bind:open={unarchive_open}
  task={unarchive_target}
  {columns}
  onConfirm={onUnarchive}
/>

<style>
  .detail-loading-indicator {
    animation: show-loading 200ms step-end both;
  }

  @keyframes show-loading {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
