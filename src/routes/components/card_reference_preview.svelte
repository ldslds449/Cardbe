<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import { getLocale } from "#lib/i18n/index.js";
  import type { Task } from "../type/task.svelte";
  import { is_date_only } from "../utils/task-date-range";

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
      .replace(/[*_`>#[\]()~-]/g, "")
      .replace(/\s+/g, " ")
      .trim();
  }

  function preview_due_time(
    due_time: Date | undefined,
    at_midnight: boolean | undefined,
  ): string | undefined {
    if (!due_time) {
      return undefined;
    }
    const date_only = is_date_only(due_time, at_midnight);
    return due_time.toLocaleString(getLocale(), {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: date_only ? undefined : "2-digit",
      minute: date_only ? undefined : "2-digit",
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
    {#if preview_due_time(task.due_time, task.due_at_midnight)}
      <div class="mt-2 text-xs">
        {m.task_due_date()}
        {preview_due_time(task.due_time, task.due_at_midnight)}
      </div>
    {/if}
    {#if preview_description(task.description)}
      <div class="mt-2 line-clamp-3 text-xs leading-5 text-muted-foreground">
        {preview_description(task.description)}
      </div>
    {/if}
  {:else}
    <div class="text-sm font-medium">{m.ui_referenced_card()}</div>
    <div class="mt-1 text-xs text-muted-foreground">
      {m.ui_open_this_link_to_view_the_card()}
    </div>
  {/if}
</div>
