<script lang="ts">
  import Calendar from "$lib/components/ui/calendar/calendar.svelte";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import CalendarIcon from "@lucide/svelte/icons/calendar";
  import XIcon from "@lucide/svelte/icons/x";
  import { parseDate } from "@internationalized/date";

  let {
    value = $bindable(""),
    label,
    min,
    max,
    invalid = false,
    describedby,
  }: {
    value?: string;
    label: string;
    min?: string;
    max?: string;
    invalid?: boolean;
    describedby?: string;
  } = $props();
  const id = $props.id();
  let open = $state(false);
</script>

<div class="grid min-w-0 gap-1">
  <label for={id} class="text-xs font-medium text-muted-foreground"
    >{label}</label
  >
  <div class="flex items-center gap-1">
    <Popover.Root bind:open>
      <Popover.Trigger>
        {#snippet child({ props })}
          <Button
            {...props}
            {id}
            variant="outline"
            class="h-9 min-w-0 flex-1 justify-start font-normal"
            aria-invalid={invalid}
            aria-describedby={describedby}
          >
            <CalendarIcon class="size-4" />
            <span class="truncate">{value || "Select date"}</span>
          </Button>
        {/snippet}
      </Popover.Trigger>
      <Popover.Content class="w-auto overflow-hidden p-0" align="start">
        <Calendar
          type="single"
          value={value ? parseDate(value) : undefined}
          minValue={min ? parseDate(min) : undefined}
          maxValue={max ? parseDate(max) : undefined}
          captionLayout="dropdown"
          initialFocus
          onValueChange={(date) => {
            value = date?.toString() ?? "";
            open = false;
          }}
        />
      </Popover.Content>
    </Popover.Root>
    {#if value}
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={`Clear ${label.toLowerCase()}`}
        onclick={() => (value = "")}><XIcon class="size-4" /></Button
      >
    {/if}
  </div>
</div>
