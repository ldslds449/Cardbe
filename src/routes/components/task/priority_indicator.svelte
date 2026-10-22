<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import ChevronsUpIcon from "@lucide/svelte/icons/chevrons-up";
  import EqualIcon from "@lucide/svelte/icons/equal";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { cn } from "$lib/utils.js";
  import { priority_label, type TaskPriority } from "../../type/task.svelte";

  let {
    priority,
    class: className,
  }: {
    priority: TaskPriority;
    class?: string;
  } = $props();

  let text = $derived(priority_label(priority));
</script>

<!-- A bordered badge with a distinct arrow per level, so it never reads like
     the due date's status-colored text beside it. Urgent is filled red and
     high is outlined red, medium is outlined in the text color, and low has a
     muted dashed outline. -->
<Badge
  variant={priority === "urgent" ? "destructive" : "outline"}
  class={cn(
    "gap-0.5 py-0 ps-1 pe-1.5 text-[11px] [&>svg]:size-3.5",
    priority === "high"
      ? "border-destructive/60 text-destructive"
      : priority === "medium"
        ? "border-foreground/40 text-foreground"
        : priority === "low" &&
          "border-dashed border-muted-foreground/50 font-normal text-muted-foreground",
    className,
  )}
  title={m.task_priority_value({ level: text })}
>
  {#if priority === "urgent"}
    <ChevronsUpIcon />
  {:else if priority === "high"}
    <ChevronUpIcon />
  {:else if priority === "medium"}
    <EqualIcon />
  {:else}
    <ChevronDownIcon />
  {/if}
  <span class="sr-only">{m.task_priority_value({ level: text })}</span>
  <span aria-hidden="true">{text}</span>
</Badge>
