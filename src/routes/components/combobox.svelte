<script lang="ts">
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
        select_placeholder = "Select...",
        search_placeholder = "Search...",
        search_label,
        selected_value = $bindable(undefined),
    }: {
        items: SelectItem[];
        select_placeholder: string;
        search_placeholder: string;
        search_label?: string;
        selected_value?: any | undefined;
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
            triggerRef.focus();
        });
    }
</script>

<Popover.Root bind:open>
    <Popover.Trigger bind:ref={triggerRef}>
        {#snippet child({ props })}
            <Button
                {...props}
                variant="outline"
                class="w-full justify-between"
                role="combobox"
                aria-expanded={open}
            >
                {selectedValue || select_placeholder}
                <ChevronsUpDownIcon class="opacity-50" />
            </Button>
        {/snippet}
    </Popover.Trigger>
    <Popover.Content class="w-full p-0">
        <Command.Root>
            <Command.Input
                placeholder={search_placeholder}
                aria-label={accessibleSearchLabel}
            />
            <Command.List>
                <Command.Empty>No found</Command.Empty>
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
                                class={cn(
                                    selected_value !== item.value &&
                                        "text-transparent",
                                )}
                            />
                            {item.label}
                        </Command.Item>
                    {/each}
                </Command.Group>
            </Command.List>
        </Command.Root>
    </Popover.Content>
</Popover.Root>
