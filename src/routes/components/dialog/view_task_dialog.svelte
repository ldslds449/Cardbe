<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatDateTime } from "$lib/i18n";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import { Button } from "$lib/components/ui/button/index.js";

  import CalendarIcon from "@lucide/svelte/icons/calendar";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import TagIcon from "@lucide/svelte/icons/tag";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";

  import { recurrence_label, type Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";
  import { is_date_only } from "../../utils/task-date-range";
  import Markdown from "../markdown.svelte";
  import ChecklistDisplay from "../task/checklist_display.svelte";
  import LabelBadge from "../label_badge.svelte";
  import ResizeHandle from "../resize_handle.svelte";
  import {
    layout_widths,
    view_dialog_width_bounds,
  } from "../../layout-widths.svelte";

  let {
    open = $bindable(false),
    task,
    onEdit,
  }: {
    open: boolean;
    task: Task | null;
    onEdit?: () => void;
  } = $props();

  function formatDate(date: Date | undefined): string {
    if (!date) {
      return m.common_not_set();
    }
    return formatDateTime(date);
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

<Dialog.Root bind:open>
  <Dialog.Content
    class="pb-0 outline-none sm:max-w-[calc(100%-2rem)]"
    style="width: {layout_widths.view_dialog_width}px"
  >
    {#each ["start", "end"] as const as edge (edge)}
      <ResizeHandle
        width={layout_widths.view_dialog_width}
        bounds={view_dialog_width_bounds}
        label={m.layout_resize_task_details()}
        {edge}
        scale={2}
        fit_viewport
        class={edge === "start" ? "-start-1.5" : "-end-1.5"}
        onresize={(width, commit) =>
          layout_widths.set_view_dialog_width(width, commit)}
      />
    {/each}
    <div class="flex max-h-[85vh] min-h-0 flex-col overflow-hidden">
      <Dialog.Header class="min-w-0 shrink-0">
        <Dialog.Title
          class="max-w-full break-normal [overflow-wrap:anywhere] pe-8"
          style="color: {task ? display_task_color(task.color) : ''};"
        >
          {task?.title ?? m.task_details()}
        </Dialog.Title>
        <Dialog.Description class="sr-only"
          >{m.task_details_label()}</Dialog.Description
        >
      </Dialog.Header>
      <ScrollArea orientation="vertical" class="min-h-0 flex-1 rounded-md">
        <div class="w-full space-y-4 py-4 ps-1 pe-4">
          {#if task}
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

            <Separator />

            <div>
              <p class="text-sm font-medium mb-1">{m.task_description()}</p>
              {#if task.description.length == 0}
                <p class="text-sm text-muted-foreground">
                  {m.task_description_empty()}
                </p>
              {:else}
                <Markdown md={task.description} />
              {/if}
            </div>
          {/if}
        </div>
      </ScrollArea>
      {#if onEdit}
        <Dialog.Footer
          class="mt-auto shrink-0 justify-center border-t bg-background/95 px-1 py-3 shadow-[0_-8px_16px_-16px_rgb(0_0_0_/_0.45)] backdrop-blur-sm sm:justify-center"
        >
          <Button
            class="min-w-40"
            onclick={() => {
              open = false;
              onEdit?.();
            }}
          >
            {m.common_edit()}
          </Button>
        </Dialog.Footer>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
