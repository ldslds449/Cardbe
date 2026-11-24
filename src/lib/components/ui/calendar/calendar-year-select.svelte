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
    WithoutChildrenOrChild<CalendarPrimitive.YearSelectProps>,
    "value" | "onchange"
  > & {
    value?: number;
    onValueChange?: (year: number) => void;
  } = $props();
</script>

<!-- bits-ui only supplies the localized year items here; the list itself is
     a shadcn-svelte Select so it scrolls inside ScrollArea instead of a
     native <select> popup. -->
<CalendarPrimitive.YearSelect bind:ref {...restProps}>
  {#snippet child({ props, yearItems, selectedYearItem })}
    {@const current = value ?? selectedYearItem?.value}
    <Select.Root
      type="single"
      value={String(current)}
      disabled={Boolean(props.disabled)}
      onValueChange={(next) => {
        const year = Number.parseInt(next, 10);
        if (!Number.isNaN(year)) {
          onValueChange?.(year);
        }
      }}
    >
      <Select.Trigger
        id={typeof props.id === "string" ? props.id : undefined}
        size="sm"
        class={cn("gap-1 ps-2 pe-1.5 font-medium", className)}
      >
        {yearItems.find((item) => item.value === current)?.label ??
          selectedYearItem?.label}
      </Select.Trigger>
      <Select.Content class="max-h-60">
        {#each yearItems as yearItem (yearItem.value)}
          <Select.Item value={String(yearItem.value)} label={yearItem.label} />
        {/each}
      </Select.Content>
    </Select.Root>
  {/snippet}
</CalendarPrimitive.YearSelect>
