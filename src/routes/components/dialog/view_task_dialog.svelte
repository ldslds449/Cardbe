<script lang="ts">
import * as Dialog from "$lib/components/ui/dialog/index.js";
import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
import { Separator } from "$lib/components/ui/separator/index.js";
import { Button } from "$lib/components/ui/button/index.js";

import CalendarIcon from "@lucide/svelte/icons/calendar";
import TagIcon from "@lucide/svelte/icons/tag";
import Repeat2Icon from "@lucide/svelte/icons/repeat-2";

import { recurrence_label, type Task } from "../../type/task.svelte";
import { display_task_color } from "../../utils/task-color";
import Markdown from "../markdown.svelte";
import ChecklistDisplay from "../task/checklist_display.svelte";
import LabelBadge from "../label_badge.svelte";

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
  if (!date) return "Not set";
  return date.toLocaleDateString() + " " + date.toLocaleTimeString();
}
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="pb-0 outline-none sm:max-w-[500px]">
    <div class="flex max-h-[85vh] min-h-0 flex-col overflow-hidden">
      <Dialog.Header class="min-w-0 shrink-0">
        <Dialog.Title
          class="max-w-full break-all pe-8"
          style="color: {task ? display_task_color(task.color) : ''};"
        >
          {task?.title ?? "Task Details"}
        </Dialog.Title>
        <Dialog.Description class="sr-only">Task details</Dialog.Description>
      </Dialog.Header>
      <ScrollArea orientation="vertical" class="min-h-0 flex-1 rounded-md">
        <div class="w-full max-w-md space-y-4 py-4 ps-1 pe-4">
          {#if task}
            <div>
              <div class="flex items-center gap-2 text-sm font-medium mb-1">
                <CalendarIcon class="size-4" />
                <span>Start Time</span>
              </div>
              <p class="text-sm text-muted-foreground">
                {formatDate(task.start_time)}
              </p>
            </div>

            {#if task.recurrence}
              <div>
                <div class="flex items-center gap-2 text-sm font-medium mb-1">
                  <Repeat2Icon class="size-4" />
                  <span>Repeat</span>
                </div>
                <p class="text-sm text-muted-foreground">
                  {recurrence_label(task.recurrence)}
                </p>
              </div>
            {/if}

            <div>
              <div class="flex items-center gap-2 text-sm font-medium mb-1">
                <CalendarIcon class="size-4" />
                <span>Due Time</span>
              </div>
              <p class="text-sm text-muted-foreground">
                {formatDate(task.due_time)}
              </p>
            </div>

            {#if task.labels && task.labels.length > 0}
              <div>
                <div class="flex items-center gap-2 text-sm font-medium mb-2">
                  <TagIcon class="size-4" />
                  <span>Labels</span>
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
                <p class="text-sm font-medium mb-2">Checklist</p>
                <ChecklistDisplay items={task.items} />
              </div>
            {/if}

            <Separator />

            <div>
              <p class="text-sm font-medium mb-1">Description</p>
              {#if task.description.length == 0}
                <p class="text-sm text-muted-foreground">No description</p>
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
            Edit
          </Button>
        </Dialog.Footer>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
