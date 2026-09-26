<script lang="ts">
import { DragDropProvider } from "@dnd-kit-svelte/svelte";
import * as Card from "$lib/components/ui/card/index.js";
import { Badge } from "$lib/components/ui/badge/index.js";

import type { Column } from "../../type/column.svelte";
import type { Task } from "../../type/task.svelte";
import CardItem from "../task/card_item.svelte";

let {
  columns,
  search_text = "",
  onViewTask = () => {},
}: {
  columns: Column[];
  search_text?: string;
  onViewTask?: (task: Task) => void;
} = $props();

function task_matches_search(task: Task): boolean {
  const search = search_text.trim().toLocaleLowerCase();
  if (!search) return true;
  return [
    task.title,
    task.description,
    ...task.labels,
    ...task.items.map((item) => item.text),
  ].some((value) => value.toLocaleLowerCase().includes(search));
}
</script>

<DragDropProvider>
  <div class="flex min-h-full min-w-max flex-row items-start gap-4 p-5">
    {#each columns as column (column.id)}
      {@const visible_tasks = column.tasks.filter(task_matches_search)}
      <Card.Root class="w-[320px] shrink-0 gap-2 bg-muted/35 py-3">
        <Card.Header class="px-4">
          <Card.Title class="flex items-center justify-between gap-3 text-base">
            <span class="truncate">{column.name}</span>
            <Badge variant="secondary" class="font-normal tabular-nums">
              {visible_tasks.length}
            </Badge>
          </Card.Title>
        </Card.Header>
        <Card.Content class="px-3">
          {#if visible_tasks.length > 0}
            {#each visible_tasks as task, task_idx (task.id)}
              <CardItem
                index={task_idx}
                {task}
                group={column.id}
                expand_content={false}
                read_only
                onViewTask={() => onViewTask(task)}
                onEditTask={() => {}}
                onDuplicateTask={() => {}}
                onDeleteTask={() => {}}
                onArchiveTask={() => {}}
              />
            {/each}
          {:else}
            <div
              class="rounded-lg border border-dashed bg-background/60 px-4 py-8 text-center text-sm text-muted-foreground"
            >
              {search_text.trim() ? "No matching tasks" : "No tasks"}
            </div>
          {/if}
        </Card.Content>
      </Card.Root>
    {/each}
  </div>
</DragDropProvider>
