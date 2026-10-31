<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatDateTime } from "#lib/i18n/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Separator } from "#lib/components/ui/separator/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Empty from "#lib/components/ui/empty/index.js";
  import type { Snippet } from "svelte";

  import CalendarIcon from "@lucide/svelte/icons/calendar";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import TagIcon from "@lucide/svelte/icons/tag";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import GaugeIcon from "@lucide/svelte/icons/gauge";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import HourglassIcon from "@lucide/svelte/icons/hourglass";
  import ListTodoIcon from "@lucide/svelte/icons/list-todo";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import Minimize2Icon from "@lucide/svelte/icons/minimize-2";

  import {
    priority_label,
    recurrence_label,
    type Task,
  } from "../../type/task.svelte";
  import type { Column } from "../../type/column.svelte";
  import TaskHistory from "../task/task_history.svelte";
  import { display_task_color } from "../../utils/task-color";
  import { is_date_only } from "../../utils/task-date-range";
  import MarkdownDocument from "../markdown_document.svelte";
  import type { TextTarget } from "../../utils/markdown-outline";
  import { request_description_reveal } from "./description-reveal";
  import ChecklistDisplay from "../task/checklist_display.svelte";
  import LabelBadge from "../label_badge.svelte";
  import { open_blockers, waiting_tasks } from "../../utils/blocking";
  import {
    layout_widths,
    task_dialog_is_split,
  } from "../../layout-widths.svelte";

  let {
    task,
    dialog_ref,
    expanded = $bindable(false),
    onEdit,
    onViewTask,
    history,
    header_actions,
  }: {
    task: Task | null;
    /** The dialog element, focused when the details open. */
    dialog_ref: HTMLDivElement | null;
    expanded?: boolean;
    onEdit?: () => void;
    /** Opens a related task, such as one this task waits on. */
    onViewTask?: (task: Task) => void;
    /** The board holding `task`; omit for tasks without history, such as templates. */
    history?: { board_id: number; columns: Column[] };
    header_actions?: Snippet;
  } = $props();

  let viewport_width = $state(globalThis.innerWidth ?? Infinity);
  const split_layout = $derived(
    task_dialog_is_split(layout_widths.task_dialog_width, viewport_width),
  );

  export function focus_initial() {
    dialog_ref?.focus();
  }

  function formatDate(date: Date | undefined): string {
    if (!date) {
      return m.common_not_set();
    }
    return formatDateTime(date);
  }

  function edit_at(target: TextTarget) {
    if (task) {
      request_description_reveal(task.id, target);
    }
    onEdit?.();
  }

  // Date-only start and due values have no meaningful time to show.
  function formatTaskDate(
    date: Date | undefined,
    at_midnight: boolean | undefined,
  ): string {
    if (date && is_date_only(date, at_midnight)) {
      return formatDateTime(date, { dateStyle: "medium" });
    }
    return formatDate(date);
  }
</script>

<svelte:window bind:innerWidth={viewport_width} />

<!-- Split and expanded details match the task editor height so switching keeps the frame. -->
<div
  class={[
    "flex max-h-[85vh] min-h-0 flex-col overflow-hidden",
    task && (split_layout || expanded) && "h-[min(85vh,56rem)]",
  ]}
>
  <Dialog.Header class="min-w-0 shrink-0 border-b pb-4">
    <div class="flex items-center gap-3 pe-8">
      <Dialog.Title
        class="min-w-0 flex-1 break-normal [overflow-wrap:anywhere]"
        style="color: {task ? display_task_color(task.color) : ''};"
      >
        {task?.title ?? m.task_details()}
      </Dialog.Title>
      {@render header_actions?.()}
    </div>
    <Dialog.Description class="sr-only"
      >{m.task_details_label()}</Dialog.Description
    >
  </Dialog.Header>
  {#if task && expanded}
    <div class="flex min-h-0 min-w-0 flex-1 flex-col pt-3">
      {#if task.description.length > 0}
        {@render task_description(task, true)}
      {:else}
        <div class="flex min-h-9 items-center justify-between gap-2 border-b">
          <p class="text-sm font-medium">{m.task_description()}</p>
          {@render expand_toggle()}
        </div>
        {@render empty_description()}
      {/if}
    </div>
  {:else if task && split_layout}
    <div class="flex min-h-0 flex-1 gap-6">
      <ScrollArea orientation="vertical" class="min-h-0 w-72 shrink-0 border-e">
        <div class="space-y-4 py-4 ps-1 pe-6">
          {@render task_details(task)}
        </div>
      </ScrollArea>
      {#if task.description.length > 0}
        <div class="flex min-h-0 min-w-0 flex-1 flex-col pt-3">
          {@render task_description(task, true)}
        </div>
      {:else}
        {@render empty_description()}
      {/if}
    </div>
  {:else}
    <ScrollArea orientation="vertical" class="min-h-0 flex-1 rounded-md">
      <div class="w-full space-y-4 py-4 ps-1 pe-4">
        {#if task}
          {@render task_details(task)}
          <Separator />
          {@render task_description(task)}
        {/if}
      </div>
    </ScrollArea>
  {/if}
  {#if onEdit}
    <Dialog.Footer
      class="mt-auto shrink-0 justify-center border-t bg-background/95 px-1 py-3 shadow-[0_-8px_16px_-16px_rgb(0_0_0_/_0.45)] backdrop-blur-sm sm:justify-center"
    >
      <Button class="min-w-40" onclick={() => onEdit?.()}>
        {m.common_edit()}
      </Button>
    </Dialog.Footer>
  {/if}
</div>

{#snippet expand_toggle()}
  <Button
    type="button"
    variant="ghost"
    size="sm"
    class="h-7 gap-1.5 px-2 text-xs"
    aria-label={expanded
      ? m.task_description_collapse()
      : m.task_description_expand()}
    title={expanded
      ? m.task_description_collapse()
      : m.task_description_expand()}
    onclick={() => (expanded = !expanded)}
  >
    {#if expanded}
      <Minimize2Icon class="size-3.5" />
      <span class="hidden sm:inline">{m.editor_collapse()}</span>
    {:else}
      <Maximize2Icon class="size-3.5" />
      <span class="hidden sm:inline">{m.editor_expand()}</span>
    {/if}
  </Button>
{/snippet}

{#snippet empty_description()}
  <Empty.Root class="my-4 min-w-0 flex-1 border border-dashed">
    <Empty.Header>
      <Empty.Media variant="icon">
        <FileTextIcon />
      </Empty.Media>
      <Empty.Title>{m.task_description_empty()}</Empty.Title>
      <Empty.Description>
        {m.task_description_empty_hint()}
      </Empty.Description>
    </Empty.Header>
  </Empty.Root>
{/snippet}

{#snippet task_details(task: Task)}
  <div>
    <div class="flex items-center gap-2 text-sm font-medium mb-1">
      <CalendarIcon class="size-4" />
      <span>{m.task_start_time()}</span>
    </div>
    <p class="text-sm text-muted-foreground">
      {formatTaskDate(task.start_time, task.start_at_midnight)}
    </p>
  </div>

  {#if task.recurrence}
    <div>
      <div class="flex items-center gap-2 text-sm font-medium mb-1">
        <Repeat2Icon class="size-4" />
        <span>{m.task_repeat()}</span>
      </div>
      <p class="text-sm text-muted-foreground">
        {recurrence_label(task.recurrence)}
      </p>
    </div>
  {/if}

  <div>
    <div class="flex items-center gap-2 text-sm font-medium mb-1">
      <CalendarIcon class="size-4" />
      <span>{m.task_due_time()}</span>
    </div>
    <p class="text-sm text-muted-foreground">
      {formatTaskDate(task.due_time, task.due_at_midnight)}
    </p>
  </div>

  {#if task.priority}
    <div>
      <div class="flex items-center gap-2 text-sm font-medium mb-1">
        <GaugeIcon class="size-4" />
        <span>{m.task_priority()}</span>
      </div>
      <p class="text-sm text-muted-foreground">
        {priority_label(task.priority)}
      </p>
    </div>
  {/if}

  {#if task.created_time}
    <div>
      <div class="flex items-center gap-2 text-sm font-medium mb-1">
        <ClockIcon class="size-4" />
        <span>{m.task_created_time()}</span>
      </div>
      <p class="text-sm text-muted-foreground">
        {formatDate(task.created_time)}
      </p>
    </div>
  {/if}

  {#if task.completed_at}
    <div>
      <div class="flex items-center gap-2 text-sm font-medium mb-1">
        <CircleCheckIcon class="size-4" />
        <span>{m.task_completed_time()}</span>
      </div>
      <p class="text-sm text-muted-foreground">
        {formatDate(task.completed_at)}
      </p>
    </div>
  {/if}

  {#if history}
    {@render related_tasks(
      m.task_waiting_on(),
      HourglassIcon,
      open_blockers(task, history.columns),
    )}
    {@render related_tasks(
      m.task_waited_on_by(),
      ListTodoIcon,
      waiting_tasks(task, history.columns),
    )}
  {/if}

  {#if task.labels && task.labels.length > 0}
    <div>
      <div class="flex items-center gap-2 text-sm font-medium mb-2">
        <TagIcon class="size-4" />
        <span>{m.task_labels()}</span>
      </div>
      <div class="flex flex-wrap gap-2">
        {#each task.labels as label}
          <LabelBadge {label} />
        {/each}
      </div>
    </div>
  {/if}

  {#if task.items.length > 0}
    <Separator />
    <div>
      <p class="text-sm font-medium mb-2">{m.task_checklist()}</p>
      <ChecklistDisplay items={task.items} />
    </div>
  {/if}

  {#if history}
    <Separator />
    <TaskHistory
      board_id={history.board_id}
      task_id={task.id}
      columns={history.columns}
    />
  {/if}
{/snippet}

{#snippet related_tasks(
  title: string,
  Icon: typeof HourglassIcon,
  tasks: Task[],
)}
  {#if tasks.length > 0}
    <div>
      <div class="flex items-center gap-2 text-sm font-medium mb-1">
        <Icon class="size-4" />
        <span>{title}</span>
      </div>
      <ul class="space-y-0.5">
        {#each tasks as related (related.id)}
          <li>
            {#if onViewTask}
              <Button
                variant="link"
                class="h-auto max-w-full justify-start p-0 text-sm font-normal"
                onclick={() => onViewTask(related)}
              >
                <span class="truncate"
                  >{related.title || m.task_untitled()}</span
                >
              </Button>
            {:else}
              <span class="block truncate text-sm text-muted-foreground"
                >{related.title || m.task_untitled()}</span
              >
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}
{/snippet}

{#snippet task_description(task: Task, fill = false)}
  <div class={fill ? "flex min-h-0 min-w-0 flex-1 flex-col" : undefined}>
    {#if task.description.length == 0}
      <p class="text-sm font-medium mb-1">{m.task_description()}</p>
      <p class="text-sm text-muted-foreground">
        {m.task_description_empty()}
      </p>
    {:else}
      <MarkdownDocument
        md={task.description}
        label={m.task_description()}
        onedit={onEdit ? edit_at : undefined}
        actions={expand_toggle}
        {fill}
      />
    {/if}
  </div>
{/snippet}
