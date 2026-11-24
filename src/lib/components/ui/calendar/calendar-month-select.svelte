<script lang="ts">
  import { Calendar as CalendarPrimitive } from "bits-ui";
  import * as Select from "#lib/components/ui/select/index.js";
  import { cn, type WithoutChildrenOrChild } from "#lib/utils.js";

  let {
    ref = $bindable(null),
    class: className,
    value,
    onValueChange,
    ...restProps
  }: Omit<
    WithoutChildrenOrChild<CalendarPrimitive.MonthSelectProps>,
    "value" | "onchange"
  > & {
    value?: number;
    onValueChange?: (month: number) => void;
  } = $props();
</script>

<!-- bits-ui only supplies the localized month items here; the list itself is
     a shadcn-svelte Select so it scrolls inside ScrollArea instead of a
     native <select> popup. -->
<CalendarPrimitive.MonthSelect bind:ref {...restProps}>
  {#snippet child({ props, monthItems, selectedMonthItem })}
    {@const current = value ?? selectedMonthItem?.value}
    <Select.Root
      type="single"
      value={String(current)}
      disabled={Boolean(props.disabled)}
      onValueChange={(next) => {
        const month = Number.parseInt(next, 10);
        if (!Number.isNaN(month)) {
          onValueChange?.(month);
        }
      }}
    >
      <Select.Trigger
        id={typeof props.id === "string" ? props.id : undefined}
        size="sm"
        class={cn("relative gap-1 ps-2 pe-1.5 font-medium", className)}
      >
        {monthItems.find((item) => item.value === current)?.label ??
          selectedMonthItem?.label}
      </Select.Trigger>
      <Select.Content class="max-h-60">
        {#each monthItems as monthItem (monthItem.value)}
          <Select.Item
            value={String(monthItem.value)}
            label={monthItem.label}
          />
        {/each}
      </Select.Content>
    </Select.Root>
  {/snippet}
</CalendarPrimitive.MonthSelect>
