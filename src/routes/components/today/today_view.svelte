<script lang="ts">
  import ChecklistSummary from "../task/checklist_summary.svelte";
  import TaskActionItems from "../task/task_action_items.svelte";
  import { formatDate, formatNumber } from "$lib/i18n";
  import { getLocale } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import { onDestroy, onMount } from "svelte";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";

  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import Share2Icon from "@lucide/svelte/icons/share-2";

  import type { Column } from "../../type/column.svelte";
  import type { Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";
  import { is_date_only } from "../../utils/task-date-range";
  import { LazyMount } from "../../utils/lazy-mount.svelte";
  import DeleteTaskDialog from "../task/delete_task_dialog.svelte";
  import LabelBadge from "../label_badge.svelte";
  import PriorityIndicator from "../task/priority_indicator.svelte";
  import {
    build_today_groups,
    today_task_count,
    type TodayTask,
  } from "./today";

  interface TodayViewProps {
    columns: Column[];
    search_text?: string;
    search_task_ids?: ReadonlySet<string> | null;
    onViewTask?: (task: Task) => void;
    onEditTask?: (task: Task) => void;
    onDuplicateTask?: (task: Task) => void;
    onSaveAsTemplate?: (task: Task) => void;
    onExportTask?: (task: Task) => void;
    onAddTask?: (date: Date) => void;
    onArchiveTask?: (task: Task) => void;
    onDeleteTask?: (task: Task) => void;
    read_only?: boolean;
  }

  let {
    columns,
    search_text = "",
    search_task_ids = null,
    onViewTask = () => {},
    onEditTask = () => {},
    onDuplicateTask = () => {},
    onSaveAsTemplate = () => {},
    onExportTask = () => {},
    onAddTask = () => {},
    onArchiveTask = () => {},
    onDeleteTask = () => {},
    read_only = false,
  }: TodayViewProps = $props();

  let now = $state(new Date());
  let delete_confirm_open = $state(false);
  let delete_target = $state<Task | null>(null);
  let archive_focus_restore_target: string | null = null;
  let groups = $derived(
    build_today_groups(columns, now, search_text, search_task_ids),
  );
  let total_count = $derived(today_task_count(groups));
  let attention_count = $derived(groups.overdue.length + groups.today.length);

  // Each row mounts two menus; rows beyond the first few per section are
  // windowed to the ones near the viewport.
  const EAGER_ROWS_PER_SECTION = 15;
  const lazy_rows = new LazyMount();

  // Remember the eagerly mounted rows so moving them down a section keeps them.
  $effect(() => {
    for (const entries of Object.values(groups)) {
      for (const entry of entries.slice(0, EAGER_ROWS_PER_SECTION)) {
        lazy_rows.mount(entry.task.id);
      }
    }
  });

  onDestroy(() => lazy_rows.destroy());

  onMount(() => {
    const refresh = () => {
      now = new Date();
    };
    const interval = window.setInterval(refresh, 60_000);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", refresh);
    };
  });

  function due_label(task: Task): string {
    if (!task.due_time) {
      return m.task_due_none();
    }
    const include_year = task.due_time.getFullYear() !== now.getFullYear();
    const date = formatDate(task.due_time, {
      month: "short",
      day: "numeric",
      year: include_year ? "numeric" : undefined,
    });
    return !is_date_only(task.due_time, task.due_at_midnight)
      ? `${date}, ${formatDate(task.due_time, {
          hour: "2-digit",
          minute: "2-digit",
          hour12: false,
        })}`
      : date;
  }

  function restore_menu_focus(event: Event, entry: TodayTask) {
    if (archive_focus_restore_target === entry.task.id) {
      event.preventDefault();
    }
    archive_focus_restore_target = null;
  }

  function ask_to_delete(task: Task) {
    delete_target = task;
    delete_confirm_open = true;
  }

  function confirm_delete() {
    if (delete_target) {
      onDeleteTask(delete_target);
    }
    delete_target = null;
  }
</script>

{#snippet task_row(entry: TodayTask, tone: "overdue" | "today" | "normal")}
  <ContextMenu.Root>
    <ContextMenu.Trigger>
      {#snippet child({ props })}
        <article
          {...props}
          {@attach lazy_rows.item(entry.task.id)}
          class="group relative overflow-hidden rounded-lg border bg-card shadow-xs transition-[border-color,box-shadow] hover:border-muted-foreground/40 hover:shadow-sm"
        >
          <button
            type="button"
            class="flex w-full items-center gap-3 px-3 py-3 pr-12 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
            onclick={() => onViewTask(entry.task)}
          >
            <span
              class="h-10 w-1 shrink-0 rounded-full"
              style={`background-color: ${display_task_color(entry.task.color) || "var(--muted-foreground)"}`}
            ></span>
            <span class="min-w-0 flex-1">
              <span class="flex min-w-0 items-center gap-2">
                <span class="truncate text-sm font-semibold"
                  >{entry.task.title}</span
                >
                <Badge
                  variant="outline"
                  class="max-w-32 shrink-0 truncate font-normal"
                >
                  {entry.column.name}
                </Badge>
              </span>
              <span
                class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground"
              >
                {#if entry.task.priority}
                  <PriorityIndicator priority={entry.task.priority} />
                {/if}
                <span
                  class={`inline-flex items-center gap-1 ${
                    tone === "overdue"
                      ? "font-medium text-destructive"
                      : tone === "today"
                        ? "font-medium text-warning"
                        : ""
                  }`}
                >
                  {#if tone === "overdue"}
                    <CircleAlertIcon class="size-3.5" />
                  {:else}
                    <ClockIcon class="size-3.5" />
                  {/if}
                  {due_label(entry.task)}
                </span>
                {#if entry.task.items.length > 0}
                  <span class="inline-flex items-center gap-1">
                    <CircleCheckIcon class="size-3.5" />
                    <ChecklistSummary items={entry.task.items} />
                  </span>
                {/if}
                {#if entry.task.labels.length > 0}
                  <span class="flex min-w-0 flex-wrap gap-1">
                    {#each entry.task.labels as label (label)}
                      <LabelBadge {label} class="py-0 font-normal" />
                    {/each}
                  </span>
                {/if}
              </span>
            </span>
          </button>

          <div
            class="absolute right-2 top-1/2 -translate-y-1/2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
          >
            <DropdownMenu.Root>
              <DropdownMenu.Trigger>
                {#snippet child({ props })}
                  <Button
                    {...props}
                    variant="ghost"
                    size="icon-sm"
                    class="bg-card/90 text-muted-foreground shadow-sm"
                    aria-label={m.explorer_actions_named({
                      name: entry.task.title,
                    })}
                    title={m.task_menu()}
                  >
                    <EllipsisIcon />
                  </Button>
                {/snippet}
              </DropdownMenu.Trigger>
              <DropdownMenu.Content
                align="end"
                class="min-w-36"
                onCloseAutoFocus={(event) => restore_menu_focus(event, entry)}
              >
                {@render menu_items(entry, false)}
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </div>
        </article>
      {/snippet}
    </ContextMenu.Trigger>
    <ContextMenu.Content
      class="min-w-36"
      onCloseAutoFocus={(event) => restore_menu_focus(event, entry)}
    >
      {@render menu_items(entry, true)}
    </ContextMenu.Content>
  </ContextMenu.Root>
{/snippet}

{#snippet menu_items(entry: TodayTask, context: boolean)}
  {@const Item = context ? ContextMenu.Item : DropdownMenu.Item}
  <TaskActionItems
    {context}
    onView={() => onViewTask(entry.task)}
    onEdit={read_only ? undefined : () => onEditTask(entry.task)}
    onArchive={read_only
      ? undefined
      : () => {
          archive_focus_restore_target = entry.task.id;
          onArchiveTask(entry.task);
        }}
    onDelete={read_only ? undefined : () => ask_to_delete(entry.task)}
  >
    {#snippet extra()}
      {#if !read_only}
        <Item onclick={() => onDuplicateTask(entry.task)}>
          <CopyIcon />
          {m.task_duplicate_action()}
        </Item>
        <Item onclick={() => onSaveAsTemplate(entry.task)}>
          <LayoutTemplateIcon />
          {m.task_template_save()}
        </Item>
        <Item onclick={() => onExportTask(entry.task)}>
          <Share2Icon />
          {m.task_share()}
        </Item>
      {/if}
    {/snippet}
  </TaskActionItems>
{/snippet}

{#snippet group_section(
  title: string,
  description: string,
  entries: TodayTask[],
  tone: "overdue" | "today" | "normal",
)}
  <section class="rounded-xl border bg-muted/20 p-4" aria-label={title}>
    <header class="mb-3 flex items-start justify-between gap-3">
      <div>
        <h2 class="font-semibold tracking-tight">{title}</h2>
        <p class="text-xs text-muted-foreground">{description}</p>
      </div>
      <Badge
        variant={tone === "overdue" && entries.length > 0
          ? "destructive"
          : "secondary"}
        class={tone === "today" && entries.length > 0
          ? "border-warning/30 bg-warning/10 text-warning"
          : ""}
      >
        {formatNumber(entries.length)}
      </Badge>
    </header>
    {#if entries.length > 0}
      <div class="space-y-2">
        {#each entries as entry, index (entry.task.id)}
          {#if index < EAGER_ROWS_PER_SECTION || lazy_rows.is_mounted(entry.task.id)}
            {@render task_row(entry, tone)}
          {:else}
            <div
              class="rounded-lg border bg-card shadow-xs"
              style={`height: ${lazy_rows.height(entry.task.id) ?? 66}px;`}
              role="button"
              tabindex="0"
              aria-label={entry.task.title}
              {@attach lazy_rows.placeholder(entry.task.id)}
              onfocus={(event) =>
                void lazy_rows.mount_focused(event, entry.task.id, (row) =>
                  row.querySelector("button"),
                )}
            ></div>
          {/if}
        {/each}
      </div>
    {:else}
      <div
        class="flex min-h-24 items-center justify-center rounded-lg border border-dashed bg-background/60 px-4 text-center text-sm text-muted-foreground"
      >
        {m.today_nothing()}
      </div>
    {/if}
  </section>
{/snippet}

<div class="mx-auto w-full max-w-6xl space-y-5 p-1" aria-label={m.today_view()}>
  <header
    class="overflow-hidden rounded-2xl border bg-background p-5 shadow-sm"
  >
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div class="flex items-start gap-3">
        <div class="rounded-xl bg-primary/10 p-2.5 text-primary">
          <SparklesIcon class="size-5" />
        </div>
        <div>
          <p class="text-sm font-medium text-primary">
            {now.toLocaleDateString(getLocale(), { weekday: "long" })}
          </p>
          <h1 class="text-2xl font-semibold tracking-tight">
            {now.toLocaleDateString(getLocale(), {
              month: "long",
              day: "numeric",
              year: "numeric",
            })}
          </h1>
          <p class="mt-1 text-sm text-muted-foreground">
            {#if attention_count === 0}
              {m.today_all_done()}
            {:else}
              {m.today_attention({ count: attention_count })}
            {/if}
          </p>
        </div>
      </div>
    </div>

    <div class="mt-5 grid grid-cols-2 gap-2 sm:grid-cols-4">
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums text-destructive">
          {formatNumber(groups.overdue.length)}
        </div>
        <div class="text-xs text-muted-foreground">{m.explorer_overdue()}</div>
      </div>
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums text-warning">
          {formatNumber(groups.today.length)}
        </div>
        <div class="text-xs text-muted-foreground">{m.workspace_today()}</div>
      </div>
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums">
          {formatNumber(groups.upcoming.length)}
        </div>
        <div class="text-xs text-muted-foreground">{m.today_upcoming()}</div>
      </div>
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums">
          {formatNumber(total_count)}
        </div>
        <div class="text-xs text-muted-foreground">{m.today_visible()}</div>
      </div>
    </div>
  </header>

  {#if total_count === 0}
    <Empty.Root class="min-h-72 rounded-xl border border-dashed">
      <Empty.Header>
        <Empty.Media variant="icon"><CircleCheckIcon /></Empty.Media>
        <Empty.Title
          >{search_text.trim()
            ? m.explorer_no_matching()
            : m.today_clear()}</Empty.Title
        >
        <Empty.Description>
          {search_text.trim() ? m.today_try_search() : m.today_empty()}
        </Empty.Description>
      </Empty.Header>
      {#if !search_text.trim()}
        <Empty.Content>
          {#if !read_only}
            <Button onclick={() => onAddTask(new Date(now))}
              >{m.today_add()}</Button
            >
          {/if}
        </Empty.Content>
      {/if}
    </Empty.Root>
  {:else}
    <div class="grid items-start gap-5 lg:grid-cols-2">
      <div class="space-y-5">
        {@render group_section(
          m.workspace_today(),
          m.today_due_help(),
          groups.today,
          "today",
        )}
        {@render group_section(
          m.explorer_overdue(),
          m.today_overdue_help(),
          groups.overdue,
          "overdue",
        )}
      </div>
      <div class="space-y-5">
        {@render group_section(
          m.today_upcoming(),
          m.today_upcoming_help(),
          groups.upcoming,
          "normal",
        )}
        {@render group_section(
          m.today_unscheduled(),
          m.today_unscheduled_help(),
          groups.unscheduled,
          "normal",
        )}
      </div>
    </div>
  {/if}
</div>

<DeleteTaskDialog
  bind:open={delete_confirm_open}
  task_title={delete_target?.title}
  onConfirm={confirm_delete}
/>
