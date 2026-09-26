<script lang="ts">
import CalendarIcon from "@lucide/svelte/icons/calendar";
import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
import ChevronLeftIcon from "@lucide/svelte/icons/chevron-left";
import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
import { Button } from "$lib/components/ui/button/index.js";
import * as Popover from "$lib/components/ui/popover/index.js";

let {
  value = $bindable(),
  disabled = false,
  onValueChange,
}: {
  value: string;
  disabled?: boolean;
  onValueChange?: (value: string) => void;
} = $props();

const month_names = Array.from({ length: 12 }, (_, month) => ({
  short: new Date(2024, month, 1).toLocaleDateString("en-US", {
    month: "short",
  }),
  long: new Date(2024, month, 1).toLocaleDateString("en-US", { month: "long" }),
}));

let open = $state(false);
let display_year = $state(new Date().getFullYear());

function parse_value() {
  const match = /^(\d{4})-(\d{2})$/.exec(value);
  if (!match) return undefined;
  const year = Number(match[1]);
  const month = Number(match[2]) - 1;
  if (year < 1 || year > 9999 || month < 0 || month > 11) return undefined;
  return { year, month };
}

const selected = $derived(parse_value());
const display_value = $derived(
  selected
    ? `${month_names[selected.month].long} ${selected.year}`
    : "Select month",
);

$effect(() => {
  if (open) display_year = selected?.year ?? new Date().getFullYear();
});

function select_month(month: number) {
  value = `${display_year}-${String(month + 1).padStart(2, "0")}`;
  open = false;
  onValueChange?.(value);
}
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="outline"
        class="w-full justify-between font-normal"
        {disabled}
      >
        <span class="flex min-w-0 items-center gap-2">
          <CalendarIcon class="size-4 shrink-0 text-muted-foreground" />
          <span class="truncate">{display_value}</span>
        </span>
        <ChevronDownIcon class="size-4 shrink-0 text-muted-foreground" />
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content class="w-72 p-3" align="start">
    <div class="mb-3 flex items-center justify-between gap-2">
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Previous year"
        disabled={display_year <= 1}
        onclick={() => { display_year -= 1; }}
      >
        <ChevronLeftIcon />
      </Button>
      <div class="text-sm font-semibold" aria-live="polite">{display_year}</div>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Next year"
        disabled={display_year >= 9999}
        onclick={() => { display_year += 1; }}
      >
        <ChevronRightIcon />
      </Button>
    </div>
    <div
      class="grid grid-cols-3 gap-1"
      role="grid"
      aria-label={`Months in ${display_year}`}
    >
      {#each month_names as month, index (month.short)}
        <Button
          variant={selected?.year === display_year && selected.month === index ? "secondary" : "ghost"}
          size="sm"
          class="font-normal"
          aria-label={`${month.long} ${display_year}`}
          aria-pressed={selected?.year === display_year && selected.month === index}
          onclick={() => select_month(index)}
        >
          {month.short}
        </Button>
      {/each}
    </div>
  </Popover.Content>
</Popover.Root>
