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
    disabled = false,
  }: {
    id: string;
    /** The visible label's ID, read together with the selection. */
    label_id: string;
    /** Consecutive options with the same `group` are listed under it. */
    options: { value: T; label: string; group?: string }[];
    selected: T[];
    /** Shown when nothing is chosen. */
    empty_label: string;
    disabled?: boolean;
  } = $props();

  let groups = $derived(
    options.reduce<{ name?: string; options: typeof options }[]>(
      (groups, option) => {
        const last = groups.at(-1);
        if (last && last.name === option.group) {
          last.options.push(option);
        } else {
          groups.push({ name: option.group, options: [option] });
        }
        return groups;
      },
      [],
    ),
  );

  function toggle(value: T, checked: boolean) {
    // Keep the options' order so summaries read consistently.
    selected = options
      .map((candidate) => candidate.value)
      .filter((candidate) =>
        candidate === value ? checked : selected.includes(candidate),
      );
  }

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

{#snippet items(list: typeof options)}
  {#each list as option (option.value)}
    <DropdownMenu.CheckboxItem
      checked={selected.includes(option.value)}
      closeOnSelect={false}
      onCheckedChange={(checked) => toggle(option.value, checked)}
      >{option.label}</DropdownMenu.CheckboxItem
    >
  {/each}
{/snippet}

<DropdownMenu.Root>
  <DropdownMenu.Trigger {disabled}>
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
    {#each groups as group, index (index)}
      {#if group.name === undefined}
        {@render items(group.options)}
      {:else}
        {#if index > 0}<DropdownMenu.Separator />{/if}
        <DropdownMenu.Group>
          <DropdownMenu.GroupHeading
            class="text-xs font-medium text-muted-foreground"
            >{group.name}</DropdownMenu.GroupHeading
          >
          {@render items(group.options)}
        </DropdownMenu.Group>
      {/if}
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
