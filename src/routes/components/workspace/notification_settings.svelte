<script lang="ts">
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { m } from "$lib/paraglide/messages.js";
  import { board } from "../../board.svelte";
  import { reminderLabel, reminderMinutes } from "./notification-reminders";

  const active_board = $derived(
    board.boards.find((item) => item.id === board.active_board_id),
  );
</script>

<Field.Field orientation="horizontal">
  <Field.Content>
    <Field.Label for="settings-notify-enabled"
      >{m.notification_due_enabled()}</Field.Label
    >
  </Field.Content>
  <Switch
    id="settings-notify-enabled"
    checked={board.notify_enabled}
    disabled={board.notification_setting_updating}
    onCheckedChange={(enabled) => void board.set_notify_enabled(enabled)}
  />
</Field.Field>
<label for="settings-notify-reminder" class="text-sm font-medium"
  >{m.notification_reminder_label()}</label
>
<Select.Root
  type="single"
  value={String(board.notify_reminder_minutes)}
  onValueChange={(value) => void board.set_notify_reminder(Number(value))}
  disabled={!board.notify_enabled}
>
  <Select.Trigger id="settings-notify-reminder" class="w-full">
    {reminderLabel(board.notify_reminder_minutes)}
  </Select.Trigger>
  <Select.Content>
    {#each reminderMinutes as minutes (minutes)}
      <Select.Item value={String(minutes)}>{reminderLabel(minutes)}</Select.Item
      >
    {/each}
  </Select.Content>
</Select.Root>
<Separator />
<Field.Set>
  <Field.Legend variant="label">{m.settings_notify_columns()}</Field.Legend>
  <Field.Description
    >{active_board
      ? m.settings_notify_columns_help({ board: active_board.name })
      : m.settings_notify_columns_no_board()}</Field.Description
  >
  {#if active_board}
    <Field.Group class="gap-3">
      {#each board.columns as column (column.id)}
        <Field.Field orientation="horizontal">
          <Checkbox
            id="settings-notify-column-{column.id}"
            checked={!board.is_column_notify_muted(column.id)}
            disabled={!board.notify_enabled}
            onCheckedChange={(checked) =>
              void board.set_column_notify_muted(column.id, !checked)}
          />
          <Field.Label
            for="settings-notify-column-{column.id}"
            class="font-normal">{column.name}</Field.Label
          >
        </Field.Field>
      {:else}
        <p class="text-sm text-muted-foreground">
          {m.settings_notify_columns_empty()}
        </p>
      {/each}
    </Field.Group>
  {/if}
</Field.Set>
