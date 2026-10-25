<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import CircleIcon from "@lucide/svelte/icons/circle";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";

  import type { TaskItem } from "../../type/task.svelte";
  import ChecklistSummary from "./checklist_summary.svelte";
  import Markdown from "../markdown.svelte";

  let {
    items,
    show_items = true,
    preview_limit,
  }: {
    items: TaskItem[];
    show_items?: boolean;
    // Card preview: lists at most this many unfinished items, one line each,
    // and leaves the full checklist to the task dialog.
    preview_limit?: number;
  } = $props();

  // Completed items stay hidden until requested; the progress bar already
  // counts them, so a long checklist lists only the remaining work.
  let show_completed = $state(false);

  let incomplete_items = $derived(items.filter((item) => !item.completed));
  let completed_items = $derived(items.filter((item) => item.completed));
  let completed_count = $derived(completed_items.length);
  let progress_percentage = $derived(
    items.length === 0 ? 0 : Math.round((completed_count / items.length) * 100),
  );
  let listed_items = $derived(
    preview_limit !== undefined
      ? incomplete_items.slice(0, preview_limit)
      : show_completed
        ? [...incomplete_items, ...completed_items]
        : incomplete_items,
  );
  let remaining_count = $derived(
    preview_limit === undefined
      ? 0
      : incomplete_items.length - listed_items.length,
  );
</script>

{#if items.length > 0}
  <div class="space-y-2" aria-label={m.task_checklist_label()}>
    <div class="space-y-1.5">
      <div
        class="flex items-center justify-between gap-3 text-xs text-muted-foreground"
      >
        <span><ChecklistSummary {items} /></span>
        <span
          >{formatNumber(progress_percentage / 100, { style: "percent" })}</span
        >
      </div>
      <div
        class="h-1.5 w-full overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-label={m.task_checklist_completion()}
        aria-valuemin="0"
        aria-valuemax={items.length}
        aria-valuenow={completed_count}
        aria-valuetext={m.task_checklist_progress_accessible({
          completed: completed_count,
          total: items.length,
        })}
      >
        <div
          class="h-full rounded-full bg-primary transition-[width] duration-300"
          style:width={`${progress_percentage}%`}
        ></div>
      </div>
    </div>
    {#if show_items}
      {#if listed_items.length > 0}
        <div class="space-y-1.5">
          {#each listed_items as item (item.id)}
            <div class="flex min-w-0 items-start gap-2 text-sm">
              {#if item.completed}
                <CircleCheckIcon
                  class="mt-0.5 size-4 shrink-0 text-muted-foreground"
                />
              {:else}
                <CircleIcon
                  class="mt-0.5 size-4 shrink-0 text-muted-foreground"
                />
              {/if}
              <div
                class="min-w-0 break-words [&_p]:inline"
                class:line-clamp-1={preview_limit !== undefined}
                class:line-through={item.completed}
                class:text-muted-foreground={item.completed}
              >
                <Markdown md={item.text} compact />
              </div>
            </div>
          {/each}
        </div>
      {/if}
      {#if remaining_count > 0}
        <p class="pl-6 text-xs text-muted-foreground">
          {m.task_checklist_more_items({ count: remaining_count })}
        </p>
      {/if}
      {#if preview_limit === undefined && completed_count > 0}
        <Button
          type="button"
          variant="ghost"
          size="xs"
          class="text-muted-foreground"
          aria-pressed={show_completed}
          onclick={() => (show_completed = !show_completed)}
        >
          <CircleCheckIcon />
          {show_completed
            ? m.task_checklist_hide_completed()
            : m.task_checklist_show_completed({ count: completed_count })}
        </Button>
      {/if}
    {/if}
  </div>
{/if}
