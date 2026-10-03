<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatNumber } from "$lib/i18n";
  import CircleIcon from "@lucide/svelte/icons/circle";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";

  import type { TaskItem } from "../../type/task.svelte";
  import Markdown from "../markdown.svelte";

  let {
    items,
    show_items = true,
  }: {
    items: TaskItem[];
    show_items?: boolean;
  } = $props();

  let completed_count = $derived(items.filter((item) => item.completed).length);
  let progress_percentage = $derived(
    items.length === 0 ? 0 : Math.round((completed_count / items.length) * 100),
  );
</script>

{#if items.length > 0}
  <div class="space-y-2" aria-label={m.task_checklist_label()}>
    <div class="space-y-1.5">
      <div
        class="flex items-center justify-between gap-3 text-xs text-muted-foreground"
      >
        <span
          >{m.task_checklist_progress({
            completed: completed_count,
            total: items.length,
          })}</span
        >
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
      <div class="space-y-1.5">
        {#each items as item (item.id)}
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
              class:line-through={item.completed}
              class:text-muted-foreground={item.completed}
            >
              <Markdown md={item.text} compact />
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
