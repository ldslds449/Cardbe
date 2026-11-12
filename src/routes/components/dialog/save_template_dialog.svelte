<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as Field from "#lib/components/ui/field/index.js";
  import { Input } from "#lib/components/ui/input/index.js";

  let {
    open = $bindable(false),
    name = $bindable(""),
    saving,
    onSave,
  }: {
    open: boolean;
    name: string;
    saving: boolean;
    onSave: () => void | Promise<void>;
  } = $props();
</script>

<Dialog.Root bind:open draft={name} busy={saving}>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>{m.ui_save_task_template()}</Dialog.Title>
      <Dialog.Description>{m.template_kept_fields()}</Dialog.Description>
    </Dialog.Header>
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void onSave();
      }}
    >
      <Field.Set>
        <Field.Field>
          <Field.FieldLabel for="template-name"
            >{m.task_template_name()}</Field.FieldLabel
          >
          <Input id="template-name" bind:value={name} required />
          <Field.Description>{m.template_reset_fields()}</Field.Description>
        </Field.Field>
        <Dialog.Footer>
          <Button type="submit" disabled={!name.trim() || saving}>
            {saving ? m.common_saving() : m.ui_save_template()}
          </Button>
        </Dialog.Footer>
      </Field.Set>
    </form>
  </Dialog.Content>
</Dialog.Root>
