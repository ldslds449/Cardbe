<script lang="ts">
import Calendar from "$lib/components/ui/calendar/calendar.svelte";
import * as Popover from "$lib/components/ui/popover/index.js";
import { Button } from "$lib/components/ui/button/index.js";
import { TimePicker } from "$lib/components/ui/time-picker/index.js";
import CalendarIcon from "@lucide/svelte/icons/calendar";
import ClockIcon from "@lucide/svelte/icons/clock";
import XIcon from "@lucide/svelte/icons/x";
import {
  CalendarDate,
  getLocalTimeZone,
  Time,
  today,
  type DateValue,
} from "@internationalized/date";

const id = $props.id();

let { value = $bindable<Date | undefined>() }: { value?: Date } = $props();

type DueDateMode = "none" | "date" | "datetime";

let date = $state<DateValue>(today(getLocalTimeZone()));
let time = $state<Time>(new Time());
let mode = $state<DueDateMode>("none");
let date_open = $state(false);
let time_open = $state(false);
let initialized = false;
let last_written_timestamp = $state<number | undefined>(undefined);

function date_has_time(value: Date): boolean {
  return (
    value.getHours() !== 0 ||
    value.getMinutes() !== 0 ||
    value.getSeconds() !== 0 ||
    value.getMilliseconds() !== 0
  );
}

function write_value(next_value: Date | undefined) {
  last_written_timestamp = next_value?.getTime();
  value = next_value;
}

function value_from_parts(
  next_date: DateValue,
  next_time: Time,
  next_mode: DueDateMode,
): Date | undefined {
  if (next_mode === "none") return undefined;

  return new Date(
    next_date.year,
    next_date.month - 1,
    next_date.day,
    next_mode === "datetime" ? next_time.hour : 0,
    next_mode === "datetime" ? next_time.minute : 0,
    next_mode === "datetime" ? next_time.second : 0,
    next_mode === "datetime" ? next_time.millisecond : 0,
  );
}

$effect(() => {
  const timestamp = value?.getTime();
  if (initialized && timestamp === last_written_timestamp) return;
  initialized = true;
  last_written_timestamp = timestamp;

  if (!value) {
    mode = "none";
    return;
  }

  date = new CalendarDate(
    value.getFullYear(),
    value.getMonth() + 1,
    value.getDate(),
  );
  time = new Time(
    value.getHours(),
    Math.floor(value.getMinutes() / 5) * 5,
    0,
    0,
  );
  mode = date_has_time(value) ? "datetime" : "date";
});

$effect(() => {
  if (!value || mode === "none") return;

  const next_value = value_from_parts(date, time, mode);
  if (!next_value) return;

  if (value.getTime() !== next_value.getTime()) {
    write_value(next_value);
  }
});

function set_due_date_mode(next_mode: DueDateMode) {
  if (next_mode === "none") {
    mode = "none";
    write_value(undefined);
    return;
  }

  const next_date = value ? date : today(getLocalTimeZone());
  const next_time = next_mode === "date" ? new Time() : time;
  mode = next_mode;
  date = next_date;
  time = next_time;
  write_value(value_from_parts(next_date, next_time, next_mode));
}

function set_date(next_date: DateValue | undefined) {
  if (!next_date) return;

  const next_mode = mode === "none" ? "date" : mode;
  date = next_date;
  mode = next_mode;
  write_value(value_from_parts(next_date, time, next_mode));
  date_open = false;
}

function due_date_label(): string {
  if (!value || mode === "none") return "No due date";

  return date.toDate(getLocalTimeZone()).toLocaleDateString();
}
</script>

<div class="flex flex-wrap items-center gap-2">
  <Popover.Root bind:open={date_open}>
    <Popover.Trigger id="{id}-date">
      {#snippet child({ props })}
        <Button
          {...props}
          variant="outline"
          class={`justify-start font-normal ${
                        value
                            ? "border-primary/30 bg-primary/5 text-foreground"
                            : "border-dashed text-muted-foreground"
                    }`}
        >
          <span class="flex min-w-0 items-center gap-2">
            <CalendarIcon
              class={`size-4 shrink-0 ${value ? "text-primary" : "text-muted-foreground"}`}
            />
            <span class="truncate">{due_date_label()}</span>
          </span>
        </Button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Content class="w-auto overflow-hidden p-0" align="start">
      <Calendar
        type="single"
        bind:value={date}
        onValueChange={set_date}
        captionLayout="dropdown"
      />
    </Popover.Content>
  </Popover.Root>

  {#if value}
    <Popover.Root
      bind:open={time_open}
      onOpenChange={(next_open) => {
                if (next_open && mode === "date") set_due_date_mode("datetime");
            }}
    >
      <Popover.Trigger id="{id}-time">
        {#snippet child({ props })}
          <Button
            {...props}
            variant="outline"
            class={`justify-start font-normal ${
                            mode === "datetime"
                                ? "border-primary/30 bg-primary/5 text-foreground"
                                : "border-dashed text-muted-foreground"
                        }`}
          >
            <ClockIcon
              class={`size-4 ${
                                mode === "datetime" ? "text-primary" : "text-muted-foreground"
                            }`}
            />
            {mode === "datetime"
                            ? time.toString().slice(0, 5)
                            : "No time"}
          </Button>
        {/snippet}
      </Popover.Trigger>
      <Popover.Content class="w-auto p-3" align="start">
        <div class="space-y-3">
          <TimePicker
            bind:time
            view="dotted"
            minuteStep={5}
            selectHours={true}
            showSeconds={false}
          />
          <div class="flex justify-end">
            <Button size="sm" onclick={() => { time_open = false; }}
              >Done</Button
            >
          </div>
        </div>
      </Popover.Content>
    </Popover.Root>

    <Button
      variant="ghost"
      size="icon"
      class="text-muted-foreground"
      aria-label="Clear due date"
      onclick={() => set_due_date_mode("none")}
    >
      <XIcon class="size-4" />
    </Button>
  {/if}
</div>
