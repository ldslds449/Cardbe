<script lang="ts">
  import * as m from "$lib/paraglide/messages";
  import type { TaskItem } from "../../type/task.svelte";
  let { items }: { items: TaskItem[] } = $props();
  const completed = $derived(items.filter((item) => item.completed).length);
</script>

{#if items.length > 0}{m.task_checklist_progress({
    completed,
    total: items.length,
  })}{/if}
