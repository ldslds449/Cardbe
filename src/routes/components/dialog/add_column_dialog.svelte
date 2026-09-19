<script lang="ts">
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

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>Add Column</Dialog.Title>
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
            <Field.FieldLabel for="column-name">Name</Field.FieldLabel>
            <Input
              id="column-name"
              placeholder="Name"
              class="col-span-5"
              bind:value={name}
              required
              onfocusout={() => {
                title_error = name.length === 0;
              }}
            />
            {#if title_error}
              <Field.Error>Please enter the name</Field.Error>
            {/if}
          </Field.Field>
          <Field.Field>
            <Button disabled={name.length === 0} type="submit">Add</Button>
          </Field.Field>
        </Field.Group>
      </Field.Set>
    </form>
  </Dialog.Content>
</Dialog.Root>
