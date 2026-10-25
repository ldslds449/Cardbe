<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";

  import type { Column } from "../../type/column.svelte";
  import type { TaskSummary } from "../../type/task.svelte";
  import ChooseColumnDialog from "../dialog/choose_column_dialog.svelte";

  let {
    open = $bindable(false),
    task,
    columns,
    onConfirm,
  }: {
    open: boolean;
    task: TaskSummary | null;
    columns: Column[];
    onConfirm: (column_id: string, task_id: string) => void;
  } = $props();
</script>

<ChooseColumnDialog
  bind:open
  title={m.calendar_unarchive()}
  description={m.archive_description({
    name: task?.title || m.task_untitled(),
  })}
  confirm_label={m.archive_restore()}
  {columns}
  onConfirm={(column_id) => {
    if (task) {
      onConfirm(column_id, task.id);
    }
  }}
/>
