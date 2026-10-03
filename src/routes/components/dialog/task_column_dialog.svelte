<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
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
    title = m.task_add_card(),
    description = m.ui_select_the_column_for_the_new_card(),
    submit_label = m.ui_continue(),
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
            <Field.FieldLabel>{m.explorer_column()}</Field.FieldLabel>
            <Combobox
              items={column_items}
              select_placeholder={m.archive_choose()}
              search_placeholder={m.ui_search_column()}
              bind:selected_value={selected_column_id}
            />
            {#if selection_error}
              <Field.Error>{m.archive_required()}</Field.Error>
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
