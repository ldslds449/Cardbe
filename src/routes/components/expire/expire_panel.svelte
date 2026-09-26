<script lang="ts">
import * as Sheet from "$lib/components/ui/sheet/index.js";
import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
import { Separator } from "$lib/components/ui/separator/index.js";
import * as InputGroup from "$lib/components/ui/input-group/index.js";
import AlertCircleIcon from "@lucide/svelte/icons/alert-circle";
import SearchIcon from "@lucide/svelte/icons/search";
import XIcon from "@lucide/svelte/icons/x";

import type { Task } from "../../type/task.svelte";
import ExpiredItem from "./expired_item.svelte";
import ViewTaskDialog from "../dialog/view_task_dialog.svelte";
import { task_matches_search } from "../calendar/calendar";

let {
  open = $bindable(false),
  expired_tasks,
}: {
  open: boolean;
  expired_tasks: Task[];
} = $props();

let view_task_dialog_open = $state(false);
let view_task = $state<Task | null>(null);
let search_text = $state("");

// Group expired tasks by due date
let expired_reduced = $derived(
  expired_tasks
    .filter((task) => task_matches_search(task, search_text))
    .map((el, el_idx) => ({ el, el_idx }))
    .toSorted((a, b) => {
      const a_time = a.el.due_time?.getTime() ?? 0;
      const b_time = b.el.due_time?.getTime() ?? 0;
      return b_time - a_time;
    })
    .reduce(
      (acc, { el, el_idx }) => {
        if (!el.due_time) return acc;
        const year = el.due_time.getFullYear();
        const month = String(el.due_time.getMonth() + 1).padStart(2, "0");
        const day = String(el.due_time.getDate()).padStart(2, "0");
        const date_str = `${year}-${month}-${day}`;
        const current = acc.get(date_str) || {
          tasks: [],
          index: [],
        };

        acc.set(date_str, {
          tasks: [...current.tasks, el],
          index: [...current.index, el_idx],
        });
        return acc;
      },
      new Map<
        string,
        {
          tasks: Task[];
          index: number[];
        }
      >(),
    ),
);

function show_detail_callback(task: Task) {
  view_task = task;
  view_task_dialog_open = true;
}
</script>

<div>
  <Sheet.Root bind:open>
    <Sheet.Content
      class="h-full"
      resizable
      defaultWidth={384}
      minWidth={320}
      maxWidth={960}
      widthStorageKey="cardbe.expired-panel-width"
    >
      <Sheet.Header>
        <Sheet.Title class="flex items-center gap-2">
          <AlertCircleIcon class="size-5 text-destructive" />
          Expired Tasks
        </Sheet.Title>
        <Sheet.Description
          >Tasks that have passed their due date</Sheet.Description
        >
      </Sheet.Header>
      <div class="px-4">
        <InputGroup.Root>
          <InputGroup.Input
            placeholder="Search expired tasks"
            aria-label="Search expired tasks"
            bind:value={search_text}
          />
          <InputGroup.Addon>
            <SearchIcon />
          </InputGroup.Addon>
          {#if search_text.length > 0}
            <InputGroup.Addon align="inline-end">
              <InputGroup.Button
                aria-label="Clear expired task search"
                title="Clear search"
                size="icon-xs"
                onclick={() => {
                                    search_text = "";
                                }}
              >
                <XIcon />
              </InputGroup.Button>
            </InputGroup.Addon>
          {/if}
        </InputGroup.Root>
      </div>
      <div class="min-h-0">
        <ScrollArea class="h-full" orientation="vertical">
          {#each expired_reduced as [date, reduced_pair] (date)}
            <Separator></Separator>
            <div>
              <div class="flex justify-center p-4">
                <h3 class="scroll-m-20 text-xl font-semibold tracking-tight">
                  {date}
                </h3>
              </div>
              <div class="space-y-2 p-2">
                {#each reduced_pair.tasks as t (t)}
                  <ExpiredItem
                    task={t}
                    onShowDetail={show_detail_callback}
                  ></ExpiredItem>
                {/each}
              </div>
            </div>
          {/each}
          {#if search_text.trim() && expired_reduced.size === 0}
            <p class="p-6 text-center text-sm text-muted-foreground">
              No matching expired tasks
            </p>
          {/if}
        </ScrollArea>
      </div>
    </Sheet.Content>
  </Sheet.Root>

  <ViewTaskDialog
    bind:open={view_task_dialog_open}
    task={view_task}
  ></ViewTaskDialog>
</div>
