<script lang="ts">
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import ClockIcon from "@lucide/svelte/icons/clock";

  import { due_label, due_label_class } from "./due-label";

  // A task's due date with its urgency, as shown in Focus lists and
  // floating windows.
  let {
    due_time,
    now,
    at_midnight = false,
  }: { due_time: Date; now: Date; at_midnight?: boolean } = $props();

  const due = $derived(due_label(due_time, now, at_midnight));
</script>

<span class="inline-flex items-center gap-1 {due_label_class(due.status)}">
  {#if due.status === "overdue"}
    <CircleAlertIcon class="size-3.5 shrink-0" />
  {:else}
    <ClockIcon class="size-3.5 shrink-0" />
  {/if}
  <span>{due.text}</span>
</span>
