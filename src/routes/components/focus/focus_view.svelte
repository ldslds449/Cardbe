<script lang="ts">
  import { onMount } from "svelte";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";

  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import SparklesIcon from "@lucide/svelte/icons/sparkles";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import Share2Icon from "@lucide/svelte/icons/share-2";

  import type { Column } from "../../type/column.svelte";
  import type { Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";
  import DeleteTaskDialog from "../task/delete_task_dialog.svelte";
  import LabelBadge from "../label_badge.svelte";
  import { build_focus_groups, focus_task_count, type FocusTask } from "./focus";

  interface FocusViewProps {
    columns: Column[];
    search_text?: string;
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
    onViewTask = () => {},
    onEditTask = () => {},
    onDuplicateTask = () => {},
    onSaveAsTemplate = () => {},
    onExportTask = () => {},
    onAddTask = () => {},
    onArchiveTask = () => {},
    onDeleteTask = () => {},
    read_only = false,
  }: FocusViewProps = $props();

  let now = $state(new Date());
  let delete_confirm_open = $state(false);
  let delete_target = $state<Task | null>(null);
  let archive_focus_restore_target: string | null = null;
  let groups = $derived(build_focus_groups(columns, now, search_text));
  let total_count = $derived(focus_task_count(groups));
  let attention_count = $derived(groups.overdue.length + groups.today.length);

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
    if (!task.due_time) return "No due date";
    const include_year = task.due_time.getFullYear() !== now.getFullYear();
    const date = task.due_time.toLocaleDateString("en", {
      month: "short",
      day: "numeric",
      year: include_year ? "numeric" : undefined,
    });
    const has_time =
      task.due_time.getHours() !== 0 ||
      task.due_time.getMinutes() !== 0 ||
      task.due_time.getSeconds() !== 0;
    return has_time
      ? `${date}, ${task.due_time.toLocaleTimeString("en", {
          hour: "2-digit",
          minute: "2-digit",
          hour12: false,
        })}`
      : date;
  }

  function checklist_label(task: Task): string | undefined {
    if (task.items.length === 0) return undefined;
    const complete = task.items.filter((item) => item.completed).length;
    return `${complete}/${task.items.length}`;
  }

  function ask_to_delete(task: Task) {
    delete_target = task;
    delete_confirm_open = true;
  }

  function confirm_delete() {
    if (delete_target) onDeleteTask(delete_target);
    delete_target = null;
  }
</script>

{#snippet task_row(entry: FocusTask, tone: "overdue" | "today" | "normal")}
  <article
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
          <span class="truncate text-sm font-semibold">{entry.task.title}</span>
          <Badge variant="outline" class="max-w-32 shrink-0 truncate font-normal">
            {entry.column.name}
          </Badge>
        </span>
        <span class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
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
          {#if checklist_label(entry.task)}
            <span class="inline-flex items-center gap-1">
              <CircleCheckIcon class="size-3.5" />
              {checklist_label(entry.task)} checklist
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
              aria-label={`Actions for ${entry.task.title}`}
              title="Task actions"
            >
              <EllipsisIcon />
            </Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content
          align="end"
          class="min-w-36"
          onCloseAutoFocus={(event) => {
            if (archive_focus_restore_target === entry.task.id) event.preventDefault();
            archive_focus_restore_target = null;
          }}
        >
          <DropdownMenu.Item onclick={() => onViewTask(entry.task)}>
            <FullscreenIcon />
            View Details
          </DropdownMenu.Item>
          <DropdownMenu.Item onclick={() => onEditTask(entry.task)}>
            <PencilIcon />
            Edit Task
          </DropdownMenu.Item>
          <DropdownMenu.Item onclick={() => onDuplicateTask(entry.task)}>
            <CopyIcon />
            Duplicate Task
          </DropdownMenu.Item>
          <DropdownMenu.Item onclick={() => onSaveAsTemplate(entry.task)}>
            <LayoutTemplateIcon />
            Save as Template
          </DropdownMenu.Item>
          <DropdownMenu.Item onclick={() => onExportTask(entry.task)}>
            <Share2Icon />
            Share Task
          </DropdownMenu.Item>
          <DropdownMenu.Item
            onSelect={() => {
              archive_focus_restore_target = entry.task.id;
              onArchiveTask(entry.task);
            }}
          >
            <ArchiveIcon />
            Archive Task
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Item variant="destructive" onclick={() => ask_to_delete(entry.task)}>
            <TrashIcon />
            Delete Task
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
  </article>
{/snippet}

{#snippet group_section(
  title: string,
  description: string,
  entries: FocusTask[],
  tone: "overdue" | "today" | "normal",
)}
  <section class="rounded-xl border bg-muted/20 p-4" aria-label={title}>
    <header class="mb-3 flex items-start justify-between gap-3">
      <div>
        <h2 class="font-semibold tracking-tight">{title}</h2>
        <p class="text-xs text-muted-foreground">{description}</p>
      </div>
      <Badge
        variant={tone === "overdue" && entries.length > 0 ? "destructive" : "secondary"}
        class={tone === "today" && entries.length > 0 ? "border-warning/30 bg-warning/10 text-warning" : ""}
      >
        {entries.length}
      </Badge>
    </header>
    {#if entries.length > 0}
      <div class="space-y-2">
        {#each entries as entry (entry.task.id)}
          {@render task_row(entry, tone)}
        {/each}
      </div>
    {:else}
      <div class="flex min-h-24 items-center justify-center rounded-lg border border-dashed bg-background/60 px-4 text-center text-sm text-muted-foreground">
        Nothing here
      </div>
    {/if}
  </section>
{/snippet}

<div class="mx-auto w-full max-w-6xl space-y-5 p-1" aria-label="Today focus view">
  <header class="overflow-hidden rounded-2xl border bg-background p-5 shadow-sm">
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div class="flex items-start gap-3">
        <div class="rounded-xl bg-primary/10 p-2.5 text-primary">
          <SparklesIcon class="size-5" />
        </div>
        <div>
          <p class="text-sm font-medium text-primary">
            {now.toLocaleDateString("en", { weekday: "long" })}
          </p>
          <h1 class="text-2xl font-semibold tracking-tight">
            {now.toLocaleDateString("en", {
              month: "long",
              day: "numeric",
              year: "numeric",
            })}
          </h1>
          <p class="mt-1 text-sm text-muted-foreground">
            {#if attention_count === 0}
              You're clear for today.
            {:else}
              {attention_count} {attention_count === 1 ? "task needs" : "tasks need"} your attention.
            {/if}
          </p>
        </div>
      </div>
    </div>

    <div class="mt-5 grid grid-cols-2 gap-2 sm:grid-cols-4">
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums text-destructive">{groups.overdue.length}</div>
        <div class="text-xs text-muted-foreground">Overdue</div>
      </div>
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums text-warning">{groups.today.length}</div>
        <div class="text-xs text-muted-foreground">Today</div>
      </div>
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums">{groups.upcoming.length}</div>
        <div class="text-xs text-muted-foreground">Upcoming</div>
      </div>
      <div class="rounded-lg border bg-background/80 p-3">
        <div class="text-2xl font-semibold tabular-nums">{total_count}</div>
        <div class="text-xs text-muted-foreground">Visible tasks</div>
      </div>
    </div>
  </header>

  {#if total_count === 0}
    <Empty.Root class="min-h-72 rounded-xl border border-dashed">
      <Empty.Header>
        <Empty.Media variant="icon"><CircleCheckIcon /></Empty.Media>
        <Empty.Title>{search_text ? "No matching tasks" : "All clear"}</Empty.Title>
        <Empty.Description>
          {search_text
            ? "Try another search."
            : "There are no active tasks in your workspace."}
        </Empty.Description>
      </Empty.Header>
      {#if !search_text}
        <Empty.Content>
          {#if !read_only}<Button onclick={() => onAddTask(new Date(now))}>Add today's first task</Button>{/if}
        </Empty.Content>
      {/if}
    </Empty.Root>
  {:else}
    <div class="grid items-start gap-5 lg:grid-cols-2">
      <div class="space-y-5">
        {@render group_section(
          "Today",
          "Tasks due before the day ends",
          groups.today,
          "today",
        )}
        {@render group_section(
          "Overdue",
          "Past due tasks that need a decision",
          groups.overdue,
          "overdue",
        )}
      </div>
      <div class="space-y-5">
        {@render group_section(
          "Upcoming",
          "Everything scheduled after today",
          groups.upcoming,
          "normal",
        )}
        {@render group_section(
          "No schedule",
          "Tasks waiting for a due date",
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
