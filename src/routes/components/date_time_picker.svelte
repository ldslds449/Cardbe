<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatDate, formatDateTime, getLocale } from "#lib/i18n/index.js";
  import Calendar from "#lib/components/ui/calendar/calendar.svelte";
  import * as Popover from "#lib/components/ui/popover/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { TimePicker } from "#lib/components/ui/time-picker/index.js";
  import CalendarIcon from "@lucide/svelte/icons/calendar";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import XIcon from "@lucide/svelte/icons/x";
  import { CalendarDate, Time, type DateValue } from "@internationalized/date";
  import { is_date_only } from "../utils/task-date-range";

  const id = $props.id();

  let {
    value = $bindable<Date | undefined>(),
    at_midnight = $bindable<boolean | undefined>(),
    emptyLabel = m.task_due_none(),
    clearLabel = m.task_due_clear(),
    invalid = false,
    labelledby,
  }: {
    value?: Date;
    /** The value is a chosen 00:00 rather than a date without a time. */
    at_midnight?: boolean;
    emptyLabel?: string;
    clearLabel?: string;
    invalid?: boolean;
    /** ID of the visible field label announced before the trigger's value. */
    labelledby?: string;
  } = $props();

  const DEFAULT_TIME = new Time(9, 0);

  let open = $state(false);

  // Exact midnight means "date only"; the value is only written by user
  // actions so opening a form never changes its draft.
  const has_time = $derived(
    value !== undefined && !is_date_only(value, at_midnight),
  );
  const date = $derived(
    value
      ? new CalendarDate(
          value.getFullYear(),
          value.getMonth() + 1,
          value.getDate(),
        )
      : undefined,
  );
  const time = $derived(
    value && has_time
      ? new Time(value.getHours(), value.getMinutes())
      : DEFAULT_TIME,
  );

  function set_value(next_date: DateValue, next_time: Time | undefined) {
    value = new Date(
      next_date.year,
      next_date.month - 1,
      next_date.day,
      next_time?.hour ?? 0,
      next_time?.minute ?? 0,
    );
    at_midnight =
      next_time && next_time.hour === 0 && next_time.minute === 0
        ? true
        : undefined;
  }

  function set_date(next_date: DateValue | undefined) {
    if (!next_date) {
      return;
    }
    set_value(next_date, has_time ? time : undefined);
  }

  function set_time(next_time: Time | undefined) {
    if (date && next_time) {
      set_value(date, next_time);
    }
  }

  function label(): string {
    if (!value) {
      return emptyLabel;
    }
    const options: Intl.DateTimeFormatOptions = {
      month: "short",
      day: "numeric",
      year:
        value.getFullYear() !== new Date().getFullYear()
          ? "numeric"
          : undefined,
    };
    return has_time
      ? formatDateTime(value, {
          ...options,
          hour: "2-digit",
          minute: "2-digit",
        })
      : formatDate(value, options);
  }
</script>

<div class="flex min-w-0 items-center gap-1">
  <Popover.Root bind:open>
    <Popover.Trigger id="{id}-trigger">
      {#snippet child({ props })}
        <Button
          {...props}
          variant="outline"
          aria-invalid={invalid || undefined}
          aria-labelledby={labelledby
            ? `${labelledby} ${id}-trigger`
            : undefined}
          class={`min-w-0 flex-1 justify-start font-normal ${
            value
              ? "border-primary/30 bg-primary/5 text-foreground"
              : "border-dashed border-border/60 bg-transparent text-muted-foreground/70 shadow-none hover:text-foreground dark:bg-transparent"
          }`}
        >
          {#if has_time}
            <ClockIcon class="size-4 shrink-0 text-primary" />
          {:else}
            <CalendarIcon
              class={`size-4 shrink-0 ${value ? "text-primary" : ""}`}
            />
          {/if}
          <span class="truncate">{label()}</span>
        </Button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Content
      class="w-auto overflow-hidden p-0"
      align="start"
      collisionPadding={8}
    >
      <ScrollArea
        class="[&>[data-slot=scroll-area-viewport]]:max-h-[calc(var(--bits-popover-content-available-height)-2px)]"
      >
        <Calendar
          locale={getLocale()}
          type="single"
          value={date}
          onValueChange={set_date}
          preventDeselect
          captionLayout="dropdown"
        />
        <div class="space-y-3 border-t p-3">
          {#if value && has_time}
            <TimePicker
              bind:time={() => time, set_time}
              view="dotted"
              minuteStep={5}
              selectHours={true}
              showSeconds={false}
            />
          {/if}
          <div class="flex items-center justify-between gap-2">
            {#if value && has_time}
              <Button
                variant="ghost"
                size="sm"
                onclick={() => {
                  if (date) {
                    set_value(date, undefined);
                  }
                }}
              >
                {m.task_time_remove()}
              </Button>
            {:else}
              <Button
                variant="ghost"
                size="sm"
                disabled={!value}
                onclick={() => set_time(DEFAULT_TIME)}
              >
                <ClockIcon class="size-4" />
                {m.task_time_add()}
              </Button>
            {/if}
            <Button size="sm" onclick={() => (open = false)}
              >{m.common_done()}</Button
            >
          </div>
        </div>
      </ScrollArea>
    </Popover.Content>
  </Popover.Root>

  {#if value}
    <Button
      variant="ghost"
      size="icon"
      class="shrink-0 text-muted-foreground"
      aria-label={clearLabel}
      title={clearLabel}
      onclick={() => {
        value = undefined;
        at_midnight = undefined;
      }}
    >
      <XIcon class="size-4" />
    </Button>
  {/if}
</div>
