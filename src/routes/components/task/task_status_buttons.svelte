<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { Button } from "#lib/components/ui/button/index.js";

  import type { TaskStatusCounts } from "../focus/focus-results";
  import type { TaskFilters, TaskStatus } from "./task-filters.svelte";

  let {
    filters,
    counts,
  }: {
    filters: TaskFilters;
    /** Null while counts are unknown; the buttons then show names only. */
    counts: TaskStatusCounts | null;
  } = $props();

  const statuses: { id: TaskStatus; label: string }[] = $derived([
    { id: "active", label: m.explorer_active() },
    { id: "overdue", label: m.explorer_overdue() },
    { id: "recurring", label: m.explorer_recurring() },
    { id: "archived", label: m.explorer_archived() },
    { id: "all", label: m.explorer_all_statuses() },
  ]);
</script>

<div class="flex flex-wrap gap-2" role="group" aria-label={m.explorer_status()}>
  {#each statuses as status (status.id)}
    <Button
      variant={filters.status === status.id ? "default" : "outline"}
      size="sm"
      class="shrink-0"
      aria-pressed={filters.status === status.id}
      onclick={() => filters.set_status(status.id)}
    >
      {status.label}
      {#if counts}
        <span class="tabular-nums opacity-70"
          >{formatNumber(counts[status.id])}</span
        >
      {/if}
    </Button>
  {/each}
</div>
