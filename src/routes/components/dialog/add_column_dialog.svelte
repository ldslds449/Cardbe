<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
  import { Input } from "$lib/components/ui/input/index.js";

  let {
    open = $bindable(),
    name = $bindable(),
    title_error = $bindable(),
    onSubmit,
  }: {
    open: boolean;
    name: string;
    title_error: boolean;
    onSubmit: () => void;
  } = $props();
</script>

<Dialog.Root bind:open draft={name}>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>{m.column_create()}</Dialog.Title>
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
          <Field.Field data-invalid={title_error}>
            <Field.FieldLabel for="column-name"
              >{m.common_name()}</Field.FieldLabel
            >
            <Input
              id="column-name"
              placeholder={m.common_name()}
              class="col-span-5"
              bind:value={name}
              required
              onfocusout={() => {
                title_error = name.length === 0;
              }}
            />
            {#if title_error}
              <Field.Error>{m.column_name_required()}</Field.Error>
            {/if}
          </Field.Field>
          <Field.Field>
            <Button disabled={name.length === 0} type="submit"
              >{m.common_add()}</Button
            >
          </Field.Field>
        </Field.Group>
      </Field.Set>
    </form>
  </Dialog.Content>
</Dialog.Root>
