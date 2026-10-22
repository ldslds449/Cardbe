<script lang="ts">
  import type { Snippet } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { cn } from "$lib/utils.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";

  // Clips tall content to `max_height` pixels with a fade and a toggle that
  // appears only when the content actually overflows.
  let {
    max_height,
    expand_label,
    collapse_label,
    class: className,
    children,
  }: {
    max_height: number;
    expand_label: string;
    collapse_label: string;
    class?: string;
    children: Snippet;
  } = $props();

  let expanded = $state(false);
  let content_height = $state(0);
  const overflowing = $derived(content_height > max_height + 1);
  const clipped = $derived(!expanded && overflowing);
</script>

<div class={cn("min-w-0", className)}>
  <div
    class={cn(
      "overflow-hidden",
      clipped &&
        "[mask-image:linear-gradient(to_bottom,black_65%,transparent)]",
    )}
    style:max-height={clipped ? `${max_height}px` : undefined}
  >
    <div bind:offsetHeight={content_height}>
      {@render children()}
    </div>
  </div>
  {#if overflowing}
    <Button
      type="button"
      variant="ghost"
      size="xs"
      class="mt-1 text-muted-foreground"
      aria-expanded={expanded}
      ondblclick={(event) => event.stopPropagation()}
      onclick={(event) => {
        event.stopPropagation();
        expanded = !expanded;
      }}
    >
      {#if expanded}
        <ChevronUpIcon />
        {collapse_label}
      {:else}
        <ChevronDownIcon />
        {expand_label}
      {/if}
    </Button>
  {/if}
</div>
