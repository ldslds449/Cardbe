<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronsUpDownIcon from "@lucide/svelte/icons/chevrons-up-down";
  import { tick } from "svelte";
  import * as Command from "$lib/components/ui/command/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { cn } from "$lib/utils.js";

  interface SelectItem {
    value: any;
    label: string;
  }

  let {
    items,
    select_placeholder = m.ui_select(),
    search_placeholder = m.ui_search(),
    search_label,
    selected_value = $bindable(undefined),
    class: className,
    onselect,
  }: {
    items: SelectItem[];
    select_placeholder: string;
    search_placeholder: string;
    search_label?: string;
    selected_value?: any | undefined;
    class?: string;
    onselect?: (value: any) => void;
  } = $props();

  let open = $state(false);
  let triggerRef = $state<HTMLButtonElement>(null!);

  const selectedValue = $derived(
    items.find((item) => item.value === selected_value)?.label,
  );
  const accessibleSearchLabel = $derived(
    search_label ?? search_placeholder.replace(/\.{3}$/, ""),
  );

  // We want to refocus the trigger button when the user selects
  // an item from the list so users can continue navigating the
  // rest of the form with the keyboard.
  function closeAndFocusTrigger() {
    open = false;
    tick().then(() => {
      triggerRef.focus({ preventScroll: true });
      onselect?.(selected_value);
    });
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger bind:ref={triggerRef}>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="outline"
        class={cn("w-full justify-between", className)}
        role="combobox"
        aria-expanded={open}
        aria-label={select_placeholder}
      >
        <span class="truncate">{selectedValue || select_placeholder}</span>
        <ChevronsUpDownIcon class="opacity-50" />
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content
    class="w-64 p-0"
    onCloseAutoFocus={(event) => {
      event.preventDefault();
      triggerRef?.focus({ preventScroll: true });
    }}
  >
    <Command.Root>
      <Command.Input
        placeholder={search_placeholder}
        aria-label={accessibleSearchLabel}
      />
      <Command.List>
        <Command.Empty>{m.ui_no_found()}</Command.Empty>
        <Command.Group value="items">
          {#each items as item (item.value)}
            <Command.Item
              value={String(item.value)}
              keywords={[item.label]}
              onSelect={() => {
                selected_value = item.value;
                closeAndFocusTrigger();
              }}
            >
              <CheckIcon
                class={cn(selected_value !== item.value && "text-transparent")}
              />
              {item.label}
            </Command.Item>
          {/each}
        </Command.Group>
      </Command.List>
    </Command.Root>
  </Popover.Content>
</Popover.Root>
