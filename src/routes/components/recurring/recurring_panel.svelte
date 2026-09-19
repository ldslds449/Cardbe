<script lang="ts">
  import * as Sheet from "$lib/components/ui/sheet/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";

  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import SearchIcon from "@lucide/svelte/icons/search";
  import XIcon from "@lucide/svelte/icons/x";

  import type { Column } from "../../type/column.svelte";
  import type { Task } from "../../type/task.svelte";
  import RecurringItem from "./recurring_item.svelte";
  import { get_recurring_tasks } from "./recurring";

  let {
    open = $bindable(false),
    columns,
    onViewTask,
    onEditTask,
    onStopRepeat,
  }: {
    open: boolean;
    columns: Column[];
    onViewTask: (task: Task) => void;
    onEditTask: (task: Task) => void;
    onStopRepeat: (task: Task) => void;
  } = $props();

  let search_text = $state("");
  let recurring_tasks = $derived(get_recurring_tasks(columns, search_text));

  function leave_and(callback: (task: Task) => void, task: Task) {
    open = false;
    callback(task);
  }
</script>

<Sheet.Root bind:open>
  <Sheet.Content class="flex h-full flex-col gap-4">
    <Sheet.Header>
      <Sheet.Title class="flex items-center gap-2">
        <Repeat2Icon class="size-5 text-primary" />
        Recurring Tasks
      </Sheet.Title>
      <Sheet.Description>
        {recurring_tasks.length} active recurring
        {recurring_tasks.length === 1 ? "task" : "tasks"}
      </Sheet.Description>
    </Sheet.Header>

    <div class="px-4">
      <InputGroup.Root>
        <InputGroup.Input
          placeholder="Search recurring tasks"
          aria-label="Search recurring tasks"
          bind:value={search_text}
        />
        <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
        {#if search_text.length > 0}
          <InputGroup.Addon align="inline-end">
            <InputGroup.Button
              aria-label="Clear recurring task search"
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

    <ScrollArea class="min-h-0 flex-1" orientation="vertical">
      <div class="space-y-2 px-4 pb-4">
        {#each recurring_tasks as entry (entry.task.id)}
          <RecurringItem
            column={entry.column}
            task={entry.task}
            onView={(task) => leave_and(onViewTask, task)}
            onEdit={(task) => leave_and(onEditTask, task)}
            {onStopRepeat}
          />
        {:else}
          <Empty.Root class="border-none py-12">
            <Empty.Header>
              <Empty.Media variant="icon"><Repeat2Icon /></Empty.Media>
              <Empty.Title>
                {search_text.trim() ? "No matching recurring tasks" : "No recurring tasks"}
              </Empty.Title>
              <Empty.Description>
                {search_text.trim()
                  ? "Try another search term."
                  : "Set a repeat schedule while editing a task."}
              </Empty.Description>
            </Empty.Header>
          </Empty.Root>
        {/each}
      </div>
    </ScrollArea>
  </Sheet.Content>
</Sheet.Root>
