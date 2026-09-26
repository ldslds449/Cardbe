<script lang="ts">
import { Button } from "$lib/components/ui/button/index.js";
import * as Dialog from "$lib/components/ui/dialog/index.js";
import * as Field from "$lib/components/ui/field/index.js";

import Combobox from "../combobox.svelte";

let {
  open = $bindable(),
  selected_column_id = $bindable(),
  selection_error = $bindable(),
  column_items,
  onSubmit,
  title = "Add Card",
  description = "Select the column for the new card.",
  submit_label = "Continue",
}: {
  open: boolean;
  selected_column_id: string;
  selection_error: boolean;
  column_items: { value: string; label: string }[];
  onSubmit: () => void;
  title?: string;
  description?: string;
  submit_label?: string;
} = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      <Dialog.Description>{description}</Dialog.Description>
    </Dialog.Header>
    <form
      novalidate
      onsubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <Field.Set>
        <Field.Group>
          <Field.Field data-invalid={selection_error}>
            <Field.FieldLabel>Column</Field.FieldLabel>
            <Combobox
              items={column_items}
              select_placeholder="Select a column..."
              search_placeholder="Search column..."
              bind:selected_value={selected_column_id}
            />
            {#if selection_error}
              <Field.Error>Please select a column.</Field.Error>
            {/if}
          </Field.Field>
          <Field.Field>
            <Button disabled={selected_column_id.length === 0} type="submit"
              >{submit_label}</Button
            >
          </Field.Field>
        </Field.Group>
      </Field.Set>
    </form>
  </Dialog.Content>
</Dialog.Root>
