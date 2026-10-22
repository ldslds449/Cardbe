<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import TableOfContentsIcon from "@lucide/svelte/icons/table-of-contents";

  import type { MarkdownHeading } from "../utils/markdown-outline";

  // `onselect` runs once the menu has closed; returning true keeps the focus
  // it moved instead of returning focus to the trigger.
  let {
    outline,
    onselect,
  }: {
    outline: MarkdownHeading[];
    onselect: (heading: MarkdownHeading) => boolean | void;
  } = $props();

  const top_level = $derived(
    Math.min(...outline.map((heading) => heading.level)),
  );
  let selected: MarkdownHeading | null = null;
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        type="button"
        variant="ghost"
        size="xs"
        class="text-muted-foreground"
      >
        <TableOfContentsIcon />
        {m.markdown_outline()}
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content
    align="end"
    class="w-72 max-h-80"
    onCloseAutoFocus={(event) => {
      const heading = selected;
      selected = null;
      if (heading && onselect(heading)) {
        event.preventDefault();
      }
    }}
  >
    {#each outline as heading (heading.start)}
      <DropdownMenu.Item
        style="padding-inline-start: {0.5 +
          (heading.level - top_level) * 0.75}rem"
        onSelect={() => (selected = heading)}
      >
        <span class="truncate">{heading.text}</span>
      </DropdownMenu.Item>
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
