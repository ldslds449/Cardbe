<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import EyeIcon from "@lucide/svelte/icons/eye";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import TrashIcon from "@lucide/svelte/icons/trash-2";

  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
  import { Input } from "$lib/components/ui/input/index.js";

  import type { Task, TaskTemplate } from "../../type/task.svelte";
  import Combobox from "../combobox.svelte";

  let {
    open = $bindable(),
    save_open = $bindable(),
    selected_template_id = $bindable(),
    template_column_id = $bindable(),
    selection_error = $bindable(),
    save_template_name = $bindable(),
    saving_template,
    templates,
    column_items,
    onUseTemplate,
    onPreviewTemplate,
    onEditTemplate,
    onDeleteTemplate,
    onSaveTemplate,
  }: {
    open: boolean;
    save_open: boolean;
    selected_template_id: string;
    template_column_id: string;
    selection_error: boolean;
    save_template_name: string;
    saving_template: boolean;
    templates: TaskTemplate[];
    column_items: { value: string; label: string }[];
    onUseTemplate: () => void;
    onPreviewTemplate: (task: Task) => void;
    onEditTemplate: (template_id: number) => void;
    onDeleteTemplate: (template_id: number) => void | Promise<unknown>;
    onSaveTemplate: () => void | Promise<void>;
  } = $props();

  const template_items = $derived(
    templates.map((template) => ({
      value: template.id.toString(),
      label: template.name,
    })),
  );
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-[520px]">
    <Dialog.Header>
      <Dialog.Title>{m.ui_task_templates()}</Dialog.Title>
      <Dialog.Description>
        {m.template_use_description()}
      </Dialog.Description>
    </Dialog.Header>
    {#if templates.length === 0}
      <Empty.Root class="py-8">
        <Empty.Header>
          <Empty.Media variant="icon"><LayoutTemplateIcon /></Empty.Media>
          <Empty.Title>{m.ui_no_task_templates()}</Empty.Title>
          <Empty.Description>
            {m.ui_open_a_card_s_action_menu_and_choose_save_as_template()}
          </Empty.Description>
        </Empty.Header>
      </Empty.Root>
    {:else}
      <form
        novalidate
        onsubmit={(event) => {
          event.preventDefault();
          onUseTemplate();
        }}
      >
        <Field.Set>
          <Field.Group>
            <Field.Field data-invalid={selection_error}>
              <Field.FieldLabel>{m.ui_template()}</Field.FieldLabel>
              <Combobox
                items={template_items}
                select_placeholder={m.ui_select_a_template()}
                search_placeholder={m.task_template_search_placeholder()}
                bind:selected_value={selected_template_id}
              />
            </Field.Field>
            <Field.Field data-invalid={selection_error}>
              <Field.FieldLabel>{m.explorer_column()}</Field.FieldLabel>
              <Combobox
                items={column_items}
                select_placeholder={m.archive_choose()}
                search_placeholder={m.archive_search()}
                bind:selected_value={template_column_id}
              />
              {#if selection_error}
                <Field.Error
                  >{m.ui_please_select_a_template_and_column()}</Field.Error
                >
              {/if}
            </Field.Field>
          </Field.Group>
          <div class="max-h-48 space-y-2 overflow-y-auto rounded-md border p-2">
            {#each templates as template (template.id)}
              <div
                class="flex items-center justify-between gap-3 rounded-md px-2 py-1.5 hover:bg-muted/50"
              >
                <div class="min-w-0">
                  <div class="truncate text-sm font-medium">
                    {template.name}
                  </div>
                  <div class="truncate text-xs text-muted-foreground">
                    {template.task.title}
                  </div>
                </div>
                <div class="flex shrink-0 items-center gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label={m.template_preview_named({
                      name: template.name,
                    })}
                    title={m.ui_preview_template()}
                    onclick={() => onPreviewTemplate(template.task)}
                  >
                    <EyeIcon />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label={m.template_edit_named({ name: template.name })}
                    title={m.ui_edit_template()}
                    onclick={() => onEditTemplate(template.id)}
                  >
                    <PencilIcon />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label={m.template_delete_named({
                      name: template.name,
                    })}
                    title={m.ui_delete_template()}
                    onclick={() => void onDeleteTemplate(template.id)}
                  >
                    <TrashIcon />
                  </Button>
                </div>
              </div>
            {/each}
          </div>
          <Dialog.Footer>
            <Button
              type="submit"
              disabled={!selected_template_id || !template_column_id}
            >
              {m.ui_use_template()}
            </Button>
          </Dialog.Footer>
        </Field.Set>
      </form>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={save_open}>
  <Dialog.Content class="sm:max-w-[425px]">
    <Dialog.Header>
      <Dialog.Title>{m.ui_save_task_template()}</Dialog.Title>
      <Dialog.Description>
        {m.ui_dates_and_checklist_completion_will_be_reset_when_this_template_is_used()}
      </Dialog.Description>
    </Dialog.Header>
    <form
      onsubmit={(event) => {
        event.preventDefault();
        void onSaveTemplate();
      }}
    >
      <Field.Set>
        <Field.Field>
          <Field.FieldLabel for="template-name"
            >{m.task_template_name()}</Field.FieldLabel
          >
          <Input id="template-name" bind:value={save_template_name} required />
        </Field.Field>
        <Dialog.Footer>
          <Button
            type="submit"
            disabled={!save_template_name.trim() || saving_template}
          >
            {saving_template ? m.common_saving() : m.ui_save_template()}
          </Button>
        </Dialog.Footer>
      </Field.Set>
    </form>
  </Dialog.Content>
</Dialog.Root>
