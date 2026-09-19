<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
  import { Button, buttonVariants } from "$lib/components/ui/button/index.js";

  import type { Column } from "../../type/column.svelte";
  import type { Task } from "../../type/task.svelte";
  import Combobox from "../combobox.svelte";

  let {
    open = $bindable(false),
    task,
    columns,
    onConfirm,
  }: {
    open: boolean;
    task: Task | null;
    columns: Column[];
    onConfirm: (column_id: string, task_id: string) => void;
  } = $props();

  let selected_column_id = $state("");
  let invalid_column = $state(false);
  let column_items = $derived(
    columns.map((column) => ({ value: column.id, label: column.name })),
  );

  $effect(() => {
    if (!open) {
      selected_column_id = "";
      invalid_column = false;
    }
  });

  function confirm() {
    if (!task || !selected_column_id) {
      invalid_column = true;
      return;
    }

    onConfirm(selected_column_id, task.id);
    open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>Unarchive Task</Dialog.Title>
      <Dialog.Description>
        Select the column where {task?.title ?? "this task"} should be restored.
      </Dialog.Description>
    </Dialog.Header>
    <Field.Field data-invalid={invalid_column}>
      <Field.Label>Column</Field.Label>
      <Combobox
        items={column_items}
        select_placeholder="Select a column..."
        search_placeholder="Search columns..."
        bind:selected_value={selected_column_id}
      />
      {#if invalid_column}
        <Field.Error>Please select a column.</Field.Error>
      {/if}
    </Field.Field>
    <Dialog.Footer>
      <Dialog.Close class={buttonVariants({ variant: "outline" })}>Cancel</Dialog.Close>
      <Button onclick={confirm}>Unarchive</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
