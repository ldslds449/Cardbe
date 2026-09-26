<script lang="ts">
import { Button } from "$lib/components/ui/button/index.js";
import { Badge } from "$lib/components/ui/badge/index.js";
import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
import ChevronLeftIcon from "@lucide/svelte/icons/chevron-left";
import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
import ClockIcon from "@lucide/svelte/icons/clock";
import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
import PencilIcon from "@lucide/svelte/icons/pencil";
import CopyIcon from "@lucide/svelte/icons/copy";
import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
import ArchiveIcon from "@lucide/svelte/icons/archive";
import TrashIcon from "@lucide/svelte/icons/trash-2";
import PlusIcon from "@lucide/svelte/icons/plus";
import UndoIcon from "@lucide/svelte/icons/undo-2";
import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
import SlidersHorizontalIcon from "@lucide/svelte/icons/sliders-horizontal";
import Share2Icon from "@lucide/svelte/icons/share-2";
import DownloadIcon from "@lucide/svelte/icons/download";
import { onMount } from "svelte";
import { DragDropProvider } from "@dnd-kit-svelte/svelte";

import type { Column } from "../../type/column.svelte";
import type { Archive } from "../../type/archive.svelte";
import type { Task } from "../../type/task.svelte";
import { display_task_color } from "../../utils/task-color";
import DeleteTaskDialog from "../task/delete_task_dialog.svelte";
import UnarchiveTaskDialog from "../archive/unarchive_task_dialog.svelte";
import CalendarDayDropZone from "./calendar_day_drop_zone.svelte";
import CalendarTaskDraggable from "./calendar_task_draggable.svelte";
import CalendarExportDialog from "../dialog/calendar_export_dialog.svelte";
import {
  build_calendar_days,
  count_due_tasks,
  date_key,
  reschedule_due_time,
  start_of_day,
  type CalendarViewMode,
} from "./calendar";

const initial_today = start_of_day(new Date());

interface CalendarViewProps {
  columns: Column[];
  archives?: Archive[];
  search_text?: string;
  visible_date?: Date;
  view_mode?: CalendarViewMode;
  show_archived?: boolean;
  show_recurring_previews?: boolean;
  onViewTask?: (task: Task) => void;
  onViewArchivedTask?: (task: Task) => void;
  onEditTask?: (task: Task) => void;
  onDuplicateTask?: (task: Task) => void;
  onSaveAsTemplate?: (task: Task) => void;
  onExportTask?: (task: Task) => void;
  onAddTask?: (date: Date) => void;
  onArchiveTask?: (task: Task) => void;
  onUnarchiveTask?: (column_id: string, task_id: string) => void;
  onDeleteTask?: (task: Task) => void;
  onRescheduleTask?: (task: Task, due_time: Date) => void;
  read_only?: boolean;
}

let {
  columns,
  archives = [],
  search_text = "",
  visible_date = $bindable(new Date(initial_today)),
  view_mode = $bindable("month"),
  show_archived = $bindable(false),
  show_recurring_previews = $bindable(true),
  onViewTask = () => {},
  onViewArchivedTask = () => {},
  onEditTask = () => {},
  onDuplicateTask = () => {},
  onSaveAsTemplate = () => {},
  onExportTask = () => {},
  onAddTask = () => {},
  onArchiveTask = () => {},
  onUnarchiveTask = () => {},
  onDeleteTask = () => {},
  onRescheduleTask = () => {},
  read_only = false,
}: CalendarViewProps = $props();

const weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
let today = $state(initial_today);
let delete_confirm_open = $state(false);
let delete_target = $state<Task | null>(null);
let unarchive_dialog_open = $state(false);
let unarchive_target = $state<Task | null>(null);
let archive_focus_restore_target: string | null = null;
let dragged_task = $state<Task | null>(null);
let export_dialog_open = $state(false);

function format_time(date: Date): string {
  return date.toLocaleTimeString("en", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function has_time(date: Date): boolean {
  return (
    date.getHours() !== 0 || date.getMinutes() !== 0 || date.getSeconds() !== 0
  );
}

function change_period(offset: number) {
  if (view_mode === "week") {
    const next = new Date(visible_date);
    next.setDate(next.getDate() + offset * 7);
    visible_date = next;
  } else {
    visible_date = new Date(
      visible_date.getFullYear(),
      visible_date.getMonth() + offset,
      1,
    );
  }
}

function set_view_mode(mode: CalendarViewMode) {
  if (mode === view_mode) return;
  if (
    mode === "week" &&
    visible_date.getFullYear() === today.getFullYear() &&
    visible_date.getMonth() === today.getMonth()
  ) {
    visible_date = new Date(today);
  }
  view_mode = mode;
}

function go_to_today() {
  const current_today = start_of_day(new Date());
  today = current_today;
  visible_date = new Date(current_today);
}

function refresh_today() {
  const current_today = start_of_day(new Date());
  if (date_key(current_today) !== date_key(today)) today = current_today;
}

onMount(() => {
  let midnight_timeout: number | undefined;

  function schedule_midnight_refresh() {
    const now = new Date();
    const next_day = new Date(
      now.getFullYear(),
      now.getMonth(),
      now.getDate() + 1,
    );
    midnight_timeout = window.setTimeout(
      () => {
        refresh_today();
        schedule_midnight_refresh();
      },
      next_day.getTime() - now.getTime() + 1000,
    );
  }

  function handle_visibility_change() {
    if (!document.hidden) refresh_today();
  }

  window.addEventListener("focus", refresh_today);
  document.addEventListener("visibilitychange", handle_visibility_change);
  schedule_midnight_refresh();

  return () => {
    window.removeEventListener("focus", refresh_today);
    document.removeEventListener("visibilitychange", handle_visibility_change);
    if (midnight_timeout !== undefined) window.clearTimeout(midnight_timeout);
  };
});

function ask_to_delete(task: Task) {
  delete_target = task;
  delete_confirm_open = true;
}

function confirm_delete() {
  if (delete_target) onDeleteTask(delete_target);
  delete_target = null;
}

function ask_to_unarchive(task: Task) {
  unarchive_target = task;
  unarchive_dialog_open = true;
}

function handle_drag_start(event: any) {
  const source = event.operation?.source;
  if (source?.type !== "calendar-task") return;
  const source_id = source.id?.toString() ?? "";
  const task_id = source_id.startsWith("calendar-task:")
    ? source_id.slice("calendar-task:".length).split(":", 1)[0]
    : "";
  dragged_task =
    columns
      .flatMap((column) => column.tasks)
      .find((task) => task.id === task_id) ?? null;
}

function handle_drag_end(event: any) {
  const task = dragged_task;
  dragged_task = null;
  const target_id = event.operation?.target?.id?.toString();
  if (!task?.due_time || !target_id?.startsWith("calendar-day:")) return;
  const target_key = target_id.slice("calendar-day:".length);
  const target_day = calendar_days.find((day) => day.key === target_key);
  if (!target_day || date_key(task.due_time) === target_key) return;
  onRescheduleTask(task, reschedule_due_time(task.due_time, target_day.date));
}

function format_period_label(date: Date, mode: CalendarViewMode): string {
  if (mode === "month") {
    return date.toLocaleDateString("en", { year: "numeric", month: "long" });
  }
  const start = start_of_day(date);
  start.setDate(start.getDate() - start.getDay());
  const end = new Date(start);
  end.setDate(end.getDate() + 6);
  if (start.getFullYear() !== end.getFullYear()) {
    return `${start.toLocaleDateString("en", { month: "short", day: "numeric", year: "numeric" })} - ${end.toLocaleDateString("en", { month: "short", day: "numeric", year: "numeric" })}`;
  }
  if (start.getMonth() !== end.getMonth()) {
    return `${start.toLocaleDateString("en", { month: "long", day: "numeric" })} - ${end.toLocaleDateString("en", { month: "long", day: "numeric", year: "numeric" })}`;
  }
  return `${start.toLocaleDateString("en", { month: "long", day: "numeric" })}-${end.getDate()}, ${end.getFullYear()}`;
}

let visible_period_label = $derived(
  format_period_label(visible_date, view_mode),
);

let due_task_count = $derived(
  count_due_tasks(columns, search_text, archives, show_archived),
);

let calendar_days = $derived(
  build_calendar_days(
    columns,
    visible_date,
    today,
    search_text,
    archives,
    show_archived,
    show_recurring_previews,
    view_mode,
  ),
);

let current_period_task_count = $derived(
  calendar_days.reduce(
    (count, day) => count + (day.in_current_month ? day.tasks.length : 0),
    0,
  ),
);
let current_period_preview_count = $derived(
  calendar_days.reduce(
    (count, day) =>
      count +
      (day.in_current_month
        ? day.tasks.filter((entry) => entry.preview).length
        : 0),
    0,
  ),
);
let enabled_display_option_count = $derived(
  Number(show_recurring_previews) + Number(show_archived),
);
</script>

<section
  class="flex min-w-[820px] flex-col gap-4 p-5"
  aria-label="Task due date calendar"
>
  <header
    class="sticky top-0 z-20 -mx-5 -mt-5 flex items-center justify-between gap-4 border-b bg-background/95 px-5 py-4 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85"
  >
    <div>
      <div class="flex items-center gap-2">
        <CalendarDaysIcon class="size-5 text-muted-foreground" />
        <h2 class="text-xl font-semibold tracking-tight">
          {visible_period_label}
        </h2>
      </div>
      <p class="mt-1 text-sm text-muted-foreground">
        {due_task_count} {due_task_count === 1 ? "task" : "tasks"} with due
        dates
        <span class="mx-1.5" aria-hidden="true">·</span>
        <span class="font-medium text-foreground">
          {current_period_task_count}
          {current_period_task_count === 1 ? "entry" : "entries"}
          this {view_mode}
          {#if current_period_preview_count > 0}
            <span class="font-normal text-muted-foreground">
              ({current_period_preview_count}
              recurring
              {current_period_preview_count === 1 ? "preview" : "previews"})
            </span>
          {/if}
        </span>
      </p>
      <div
        class="mt-2 flex flex-wrap items-center gap-3 text-[11px] text-muted-foreground"
        aria-label="Calendar item legend"
      >
        <span class="inline-flex items-center gap-1.5">
          <span
            class="size-2.5 rounded-sm border-2 border-foreground/60 bg-card shadow-xs"
          ></span>
          Active
        </span>
        <span class="inline-flex items-center gap-1.5 opacity-70">
          <span
            class="size-2.5 rounded-sm border border-dashed border-muted-foreground/60 bg-transparent"
          ></span>
          Recurring preview
        </span>
        <span class="inline-flex items-center gap-1.5 opacity-55">
          <span
            class="size-2.5 rounded-sm border border-dotted border-muted-foreground/50 bg-muted/30"
          ></span>
          Archived
        </span>
      </div>
    </div>

    <div class="flex items-center gap-2">
      {#if !read_only}
        <Button
          variant="outline"
          size="sm"
          class="gap-1.5"
          onclick={() => { export_dialog_open = true; }}
        >
          <DownloadIcon class="size-3.5" />
          Export
        </Button>
      {/if}
      <Button variant="outline" size="sm" onclick={go_to_today}>Today</Button>
      <div
        class="flex overflow-hidden rounded-md border bg-background shadow-xs"
      >
        <Button
          variant="ghost"
          size="icon-sm"
          class="rounded-none border-r"
          aria-label={`Previous ${view_mode}`}
          title={`Previous ${view_mode}`}
          onclick={() => change_period(-1)}
        >
          <ChevronLeftIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          class="rounded-none"
          aria-label={`Next ${view_mode}`}
          title={`Next ${view_mode}`}
          onclick={() => change_period(1)}
        >
          <ChevronRightIcon />
        </Button>
      </div>
      <div
        class="flex overflow-hidden rounded-md border bg-background shadow-xs"
        aria-label="Calendar view"
      >
        <Button
          variant={view_mode === "month" ? "secondary" : "ghost"}
          size="sm"
          class="rounded-none border-r"
          aria-pressed={view_mode === "month"}
          onclick={() => set_view_mode("month")}
          >Month</Button
        >
        <Button
          variant={view_mode === "week" ? "secondary" : "ghost"}
          size="sm"
          class="rounded-none"
          aria-pressed={view_mode === "week"}
          onclick={() => set_view_mode("week")}
          >Week</Button
        >
      </div>
      {#if !read_only}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Button {...props} variant="outline" size="sm" class="gap-1.5">
                <SlidersHorizontalIcon class="size-3.5" />
                Display
                <Badge
                  variant="secondary"
                  class="ml-0.5 h-5 min-w-5 justify-center rounded-full px-1.5 text-[10px] tabular-nums"
                  >{enabled_display_option_count}</Badge
                >
              </Button>
            {/snippet}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="end" class="min-w-48">
            <DropdownMenu.Label>Calendar items</DropdownMenu.Label>
            <DropdownMenu.Separator />
            <DropdownMenu.CheckboxItem
              bind:checked={show_recurring_previews}
              closeOnSelect={false}
            >
              Show recurring previews
            </DropdownMenu.CheckboxItem>
            <DropdownMenu.CheckboxItem
              bind:checked={show_archived}
              closeOnSelect={false}
            >
              Show archived tasks
            </DropdownMenu.CheckboxItem>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      {/if}
    </div>
  </header>

  <DragDropProvider onDragStart={handle_drag_start} onDragEnd={handle_drag_end}>
    <div
      class={`grid h-[calc(100vh-12rem)] flex-none grid-cols-7 gap-px overflow-hidden rounded-xl border bg-border shadow-sm ${view_mode === "month" ? "min-h-[56rem]" : "min-h-[18rem]"}`}
      style={`grid-template-rows: auto repeat(${view_mode === "month" ? 6 : 1}, minmax(0, 1fr));`}
    >
      {#each weekdays as weekday}
        <div
          class="bg-muted/80 px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground"
        >
          {weekday}
        </div>
      {/each}

      {#each calendar_days as day (day.key)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <CalendarDayDropZone
          day_key={day.key}
          disabled={read_only}
          class={`group/day flex min-h-0 flex-col overflow-hidden bg-background p-2 ${
          day.in_current_month ? "" : "bg-muted/30 text-muted-foreground/50"
        } ${day.is_today ? "ring-2 ring-inset ring-primary/35" : ""}`}
          title={read_only ? undefined : "Double-click empty space to add a task"}
          ondblclick={read_only ? undefined : () => onAddTask(new Date(day.date))}
        >
          <div class="mb-1.5 flex items-center justify-between">
            <span
              class={`flex size-7 items-center justify-center rounded-full text-sm tabular-nums ${
              day.is_today ? "bg-primary font-semibold text-primary-foreground" : ""
            }`}
            >
              {day.date.getDate()}
            </span>
            <div class="flex items-center gap-1">
              {#if day.tasks.length > 0}
                <span class="text-[11px] tabular-nums text-muted-foreground"
                  >{day.tasks.length}</span
                >
              {/if}
              {#if !read_only}
                <Button
                  variant="ghost"
                  size="icon-sm"
                  class="size-6 opacity-0 transition-opacity group-hover/day:opacity-100 focus-visible:opacity-100"
                  aria-label={`Add task due ${day.date.toLocaleDateString("en", {
                year: "numeric",
                month: "long",
                day: "numeric",
              })}`}
                  title="Add task"
                  onclick={(event) => {
                event.stopPropagation();
                onAddTask(new Date(day.date));
              }}
                  ondblclick={(event) => event.stopPropagation()}
                >
                  <PlusIcon />
                </Button>
              {/if}
            </div>
          </div>

          <div
            class="min-h-0 flex-1 overflow-y-auto pr-0.5 [scrollbar-width:thin]"
          >
            <div class="flex flex-col gap-1">
              {#each day.tasks as entry (`${entry.task.id}-${entry.task.due_time?.getTime()}-${entry.archived}-${entry.preview}`)}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <CalendarTaskDraggable
                  task_id={entry.task.id}
                  instance_key={`${entry.task.due_time?.getTime()}-${entry.archived}-${entry.preview}`}
                  disabled={read_only || entry.archived || entry.preview}
                  class={`group relative overflow-hidden rounded-md text-xs transition-[border-color,background-color,box-shadow,opacity] ${
                  entry.archived
                    ? "border border-dotted border-muted-foreground/40 bg-muted/30 opacity-55 shadow-none hover:opacity-70"
                    : entry.preview
                      ? "border border-dashed border-muted-foreground/45 bg-transparent opacity-70 shadow-none hover:opacity-85"
                      : "border-2 border-foreground/30 bg-card shadow-sm hover:border-foreground/50 hover:shadow-md"
                } ${!entry.archived && !entry.preview ? (read_only ? "cursor-pointer" : "cursor-grab active:cursor-grabbing select-none touch-none") : ""}`}
                  oncontextmenu={read_only ? undefined : (event) => {
                  event.preventDefault();
                  if (!entry.archived && !entry.preview) onEditTask(entry.task);
                }}
                >
                  <button
                    type="button"
                    class="flex w-full items-stretch gap-1.5 overflow-hidden px-2 py-1.5 pr-8 text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring"
                    title={`${entry.task.title} · ${entry.column.name}`}
                    onpointerup={(event) => event.currentTarget.blur()}
                    onclick={() =>
                    entry.archived
                      ? onViewArchivedTask(entry.task)
                      : onViewTask(entry.task)}
                    ondblclick={(event) => event.stopPropagation()}
                  >
                    <span
                      class="w-1 shrink-0 self-stretch rounded-full"
                      style={`background-color: ${
                      entry.archived
                        ? "var(--muted-foreground)"
                        : entry.preview
                          ? "var(--muted-foreground)"
                          : display_task_color(entry.task.color) || "var(--muted-foreground)"
                    }`}
                    ></span>
                    <span class="flex min-w-0 flex-1 flex-col justify-center">
                      <span
                        class={`min-w-0 truncate ${
                        entry.archived
                          ? "font-normal text-muted-foreground line-through"
                          : entry.preview
                            ? "font-normal text-muted-foreground"
                            : "font-semibold"
                      }`}
                        >{entry.task.title}</span
                      >
                      {#if entry.archived}
                        <Badge
                          variant="secondary"
                          class="mt-0.5 h-4 w-fit gap-1 border border-foreground/20 bg-muted px-1 text-[9px] font-normal text-muted-foreground"
                        >
                          <ArchiveIcon class="size-2.5" />
                          Archived
                        </Badge>
                      {:else if entry.preview}
                        <Badge
                          variant="secondary"
                          class="mt-0.5 h-4 w-fit gap-1 border border-dashed border-muted-foreground/30 bg-transparent px-1 text-[9px] font-normal text-muted-foreground"
                        >
                          <Repeat2Icon class="size-2.5" />
                          Preview
                        </Badge>
                      {/if}
                      {#if entry.task.due_time && has_time(entry.task.due_time)}
                        <span
                          class="mt-0.5 flex items-center gap-1 text-[10px] tabular-nums text-muted-foreground"
                        >
                          <ClockIcon class="size-2.5" />
                          {format_time(entry.task.due_time)}
                        </span>
                      {/if}
                    </span>
                  </button>
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  {#if !read_only}
                    <div
                      class="absolute right-0.5 top-1/2 -translate-y-1/2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
                      onpointerdown={(event) => event.stopPropagation()}
                      ondblclick={(event) => event.stopPropagation()}
                    >
                      <DropdownMenu.Root>
                        <DropdownMenu.Trigger>
                          {#snippet child({ props })}
                            <Button
                              {...props}
                              variant="ghost"
                              size="icon-sm"
                              class="size-6 bg-card/90 text-muted-foreground shadow-sm hover:text-foreground"
                              aria-label={`Actions for ${entry.task.title}`}
                              title="Task actions"
                            >
                              <EllipsisIcon class="size-3.5" />
                            </Button>
                          {/snippet}
                        </DropdownMenu.Trigger>
                        <DropdownMenu.Content
                          align="end"
                          class="min-w-36"
                          onCloseAutoFocus={(event) => {
                        if (archive_focus_restore_target === entry.task.id) {
                          event.preventDefault();
                        }
                        archive_focus_restore_target = null;
                      }}
                        >
                          <DropdownMenu.Item
                            onclick={() =>
                          entry.archived
                            ? onViewArchivedTask(entry.task)
                            : onViewTask(entry.task)}
                          >
                            <FullscreenIcon />
                            View Details
                          </DropdownMenu.Item>
                          {#if entry.archived}
                            <DropdownMenu.Item
                              onclick={() => ask_to_unarchive(entry.task)}
                            >
                              <UndoIcon />
                              Unarchive Task
                            </DropdownMenu.Item>
                          {:else if !entry.preview}
                            <DropdownMenu.Item
                              onclick={() => onEditTask(entry.task)}
                            >
                              <PencilIcon />
                              Edit Task
                            </DropdownMenu.Item>
                            <DropdownMenu.Item
                              onclick={() => onDuplicateTask(entry.task)}
                            >
                              <CopyIcon />
                              Duplicate Task
                            </DropdownMenu.Item>
                            <DropdownMenu.Item
                              onclick={() => onSaveAsTemplate(entry.task)}
                            >
                              <LayoutTemplateIcon />
                              Save as Template
                            </DropdownMenu.Item>
                            <DropdownMenu.Item
                              onclick={() => onExportTask(entry.task)}
                            >
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
                            <DropdownMenu.Item
                              variant="destructive"
                              onclick={() => ask_to_delete(entry.task)}
                            >
                              <TrashIcon />
                              Delete Task
                            </DropdownMenu.Item>
                          {/if}
                        </DropdownMenu.Content>
                      </DropdownMenu.Root>
                    </div>
                  {/if}
                </CalendarTaskDraggable>
              {/each}
            </div>
          </div>
        </CalendarDayDropZone>
      {/each}
    </div>
  </DragDropProvider>
</section>

{#if !read_only}
  <CalendarExportDialog
    bind:open={export_dialog_open}
    days={calendar_days}
    {columns}
    {archives}
    {search_text}
    {visible_date}
    {today}
    {show_archived}
    {show_recurring_previews}
    {view_mode}
    period_label={visible_period_label}
  />

  <DeleteTaskDialog
    bind:open={delete_confirm_open}
    task_title={delete_target?.title}
    onConfirm={confirm_delete}
  />

  <UnarchiveTaskDialog
    bind:open={unarchive_dialog_open}
    task={unarchive_target}
    {columns}
    onConfirm={onUnarchiveTask}
  />
{/if}
