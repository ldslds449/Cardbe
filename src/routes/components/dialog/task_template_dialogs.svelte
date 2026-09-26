<script lang="ts">
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
      <Dialog.Title>Task Templates</Dialog.Title>
      <Dialog.Description>
        Choose a reusable task and the column where the new card should be
        created.
      </Dialog.Description>
    </Dialog.Header>
    {#if templates.length === 0}
      <Empty.Root class="py-8">
        <Empty.Header>
          <Empty.Media variant="icon"><LayoutTemplateIcon /></Empty.Media>
          <Empty.Title>No task templates</Empty.Title>
          <Empty.Description>
            Open a card's action menu and choose “Save as Template”.
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
              <Field.FieldLabel>Template</Field.FieldLabel>
              <Combobox
                items={template_items}
                select_placeholder="Select a template..."
                search_placeholder="Search templates..."
                bind:selected_value={selected_template_id}
              />
            </Field.Field>
            <Field.Field data-invalid={selection_error}>
              <Field.FieldLabel>Column</Field.FieldLabel>
              <Combobox
                items={column_items}
                select_placeholder="Select a column..."
                search_placeholder="Search columns..."
                bind:selected_value={template_column_id}
              />
              {#if selection_error}
                <Field.Error>Please select a template and column.</Field.Error>
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
                    aria-label={`Preview template ${template.name}`}
                    title="Preview template"
                    onclick={() => onPreviewTemplate(template.task)}
                  >
                    <EyeIcon />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label={`Edit template ${template.name}`}
                    title="Edit template"
                    onclick={() => onEditTemplate(template.id)}
                  >
                    <PencilIcon />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label={`Delete template ${template.name}`}
                    title="Delete template"
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
              Use Template
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
      <Dialog.Title>Save Task Template</Dialog.Title>
      <Dialog.Description>
        Dates and checklist completion will be reset when this template is used.
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
          <Field.FieldLabel for="template-name">Template name</Field.FieldLabel>
          <Input id="template-name" bind:value={save_template_name} required />
        </Field.Field>
        <Dialog.Footer>
          <Button
            type="submit"
            disabled={!save_template_name.trim() || saving_template}
          >
            {saving_template ? "Saving..." : "Save Template"}
          </Button>
        </Dialog.Footer>
      </Field.Set>
    </form>
  </Dialog.Content>
</Dialog.Root>
