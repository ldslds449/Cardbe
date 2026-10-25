<script lang="ts" generics="T extends string | number">
  import { getLocale } from "#lib/i18n/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";

  // Any number of values; nothing chosen means no restriction.
  let {
    id,
    label_id,
    options,
    selected = $bindable([]),
    empty_label,
  }: {
    id: string;
    /** The visible label's ID, read together with the selection. */
    label_id: string;
    options: { value: T; label: string }[];
    selected: T[];
    /** Shown when nothing is chosen. */
    empty_label: string;
  } = $props();

  let summary = $derived(
    selected.length === 0
      ? empty_label
      : new Intl.ListFormat(getLocale(), { type: "unit" }).format(
          options
            .filter((option) => selected.includes(option.value))
            .map((option) => option.label),
        ),
  );
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="outline"
        class="h-9 w-full min-w-0 justify-between font-normal"
        aria-labelledby="{label_id} {id}"
      >
        <span {id} class="truncate">{summary}</span>
        <ChevronDownIcon class="size-4" />
      </Button>
    {/snippet}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content class="max-h-80" align="start">
    {#each options as option (option.value)}
      <DropdownMenu.CheckboxItem
        checked={selected.includes(option.value)}
        closeOnSelect={false}
        onCheckedChange={(checked) => {
          // Keep the options' order so summaries read consistently.
          selected = options
            .map((candidate) => candidate.value)
            .filter((value) =>
              value === option.value ? checked : selected.includes(value),
            );
        }}>{option.label}</DropdownMenu.CheckboxItem
      >
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
