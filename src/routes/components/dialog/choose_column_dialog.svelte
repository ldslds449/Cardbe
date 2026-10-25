<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as Field from "#lib/components/ui/field/index.js";
  import { Button, buttonVariants } from "#lib/components/ui/button/index.js";

  import type { Column } from "../../type/column.svelte";
  import Combobox from "../combobox.svelte";

  // Chooses where tasks go, e.g. when restoring or reopening them.
  let {
    open = $bindable(false),
    title,
    description,
    confirm_label,
    columns,
    onConfirm,
  }: {
    open: boolean;
    title: string;
    description: string;
    confirm_label: string;
    columns: Column[];
    onConfirm: (column_id: string) => void;
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
    if (!selected_column_id) {
      invalid_column = true;
      return;
    }

    onConfirm(selected_column_id);
    open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      <Dialog.Description>{description}</Dialog.Description>
    </Dialog.Header>
    <Field.Field data-invalid={invalid_column}>
      <Field.Label>{m.explorer_column()}</Field.Label>
      <Combobox
        items={column_items}
        select_placeholder={m.archive_choose()}
        search_placeholder={m.archive_search()}
        bind:selected_value={selected_column_id}
      />
      {#if invalid_column}
        <Field.Error>{m.archive_required()}</Field.Error>
      {/if}
    </Field.Field>
    <Dialog.Footer>
      <Dialog.Close class={buttonVariants({ variant: "outline" })}
        >{m.common_cancel()}</Dialog.Close
      >
      <Button onclick={confirm}>{confirm_label}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
