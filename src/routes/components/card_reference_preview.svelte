<script lang="ts">
  import type { Task } from "../type/task.svelte";

  let {
    task,
    location,
    left,
    top,
    show_below,
  }: {
    task: Task | undefined;
    location?: string;
    left: number;
    top: number;
    show_below: boolean;
  } = $props();

  function preview_description(description: string): string {
    return description
      .replace(/\[\[([^\]|]+)\|task_\d+\]\]/g, "$1")
      .replace(/[*_`>#\[\]()~-]/g, "")
      .replace(/\s+/g, " ")
      .trim();
  }

  function preview_due_time(due_time: Date | undefined): string | undefined {
    if (!due_time) return undefined;
    return due_time.toLocaleString([], {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

<div
  role="tooltip"
  class="pointer-events-none fixed z-[200] w-72 max-w-[calc(100vw-1.5rem)] rounded-lg border border-foreground/25 bg-popover p-3 text-left text-popover-foreground shadow-xl ring-1 ring-background"
  class:-translate-y-full={!show_below}
  style:left={`${left}px`}
  style:top={`${top}px`}
>
  {#if task}
    <div class="truncate text-sm font-semibold">{task.title}</div>
    <div class="mt-0.5 text-xs text-muted-foreground">{location}</div>
    {#if preview_due_time(task.due_time)}
      <div class="mt-2 text-xs">Due {preview_due_time(task.due_time)}</div>
    {/if}
    {#if preview_description(task.description)}
      <div class="mt-2 line-clamp-3 text-xs leading-5 text-muted-foreground">
        {preview_description(task.description)}
      </div>
    {/if}
  {:else}
    <div class="text-sm font-medium">Referenced card</div>
    <div class="mt-1 text-xs text-muted-foreground">
      Open this link to view the card.
    </div>
  {/if}
</div>
