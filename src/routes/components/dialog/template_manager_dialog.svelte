<script lang="ts" module>
  // The column last used for a template, kept while the app is open.
  const remembered = $state({ column_id: "" });
</script>

<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import * as AlertDialog from "#lib/components/ui/alert-dialog/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Command from "#lib/components/ui/command/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import * as Empty from "#lib/components/ui/empty/index.js";
  import * as Field from "#lib/components/ui/field/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import * as Select from "#lib/components/ui/select/index.js";

  import { untrack } from "svelte";
  import CircleIcon from "@lucide/svelte/icons/circle";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import TrashIcon from "@lucide/svelte/icons/trash-2";

  import { board } from "../../board.svelte";
  import { priority_label, type TaskTemplate } from "../../type/task.svelte";
  import type { Column } from "../../type/column.svelte";
  import { display_task_color } from "../../utils/task-color";
  import LabelBadge from "../label_badge.svelte";
  import Markdown from "../markdown.svelte";
  import CopyTemplateDialog from "./copy_template_dialog.svelte";

  let {
    open = $bindable(false),
    selected_id = $bindable(null),
    templates,
    columns,
    read_only,
    onUse,
    onCreate,
    onEdit,
    onDelete,
  }: {
    open: boolean;
    /** The previewed template; openings keep it while it still exists. */
    selected_id?: number | null;
    templates: TaskTemplate[];
    columns: Column[];
    /** Viewers can browse and copy templates but not use or change them. */
    read_only: boolean;
    onUse: (template: TaskTemplate, column_id: string) => void;
    onCreate: () => void;
    onEdit: (template: TaskTemplate) => void;
    onDelete: (template_id: number) => void | Promise<unknown>;
  } = $props();

  let column_id = $state("");
  let deleting = $state<TaskTemplate | null>(null);
  let copying = $state<{ template: TaskTemplate; source_id: number } | null>(
    null,
  );

  const selected = $derived(
    templates.find((template) => template.id === selected_id) ??
      templates[0] ??
      null,
  );
  const column_name = $derived(
    columns.find((column) => column.id === column_id)?.name,
  );

  // Each opening keeps the chosen template, or starts on the first one, and
  // uses the last used column.
  $effect(() => {
    if (!open) {
      return;
    }
    untrack(() => {
      if (!templates.some((template) => template.id === selected_id)) {
        selected_id = templates[0]?.id ?? null;
      }
      column_id = columns.some((column) => column.id === remembered.column_id)
        ? remembered.column_id
        : (columns[0]?.id ?? "");
    });
  });

  function use_selected() {
    if (!selected || !column_id) {
      return;
    }
    remembered.column_id = column_id;
    onUse(selected, column_id);
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="flex h-[min(85vh,40rem)] flex-col gap-0 p-0 sm:max-w-3xl"
  >
    <Dialog.Header class="border-b p-6 pb-4">
      <div class="flex items-center gap-3 pe-8">
        <div class="min-w-0 flex-1 space-y-1">
          <Dialog.Title>{m.ui_task_templates()}</Dialog.Title>
          <Dialog.Description
            >{m.template_manager_description()}</Dialog.Description
          >
        </div>
        {#if !read_only && templates.length > 0}
          <Button size="sm" variant="outline" onclick={onCreate}>
            <PlusIcon />
            {m.template_new()}
          </Button>
        {/if}
      </div>
    </Dialog.Header>

    {#if templates.length === 0}
      <Empty.Root class="flex-1">
        <Empty.Header>
          <Empty.Media variant="icon"><LayoutTemplateIcon /></Empty.Media>
          <Empty.Title>{m.ui_no_task_templates()}</Empty.Title>
          <Empty.Description>{m.template_empty_description()}</Empty.Description
          >
        </Empty.Header>
        {#if !read_only}
          <Empty.Content>
            <Button onclick={onCreate}>
              <PlusIcon />
              {m.template_new()}
            </Button>
          </Empty.Content>
        {/if}
      </Empty.Root>
    {:else}
      <div
        class="grid min-h-0 flex-1 grid-rows-[minmax(0,12rem)_minmax(0,1fr)] sm:grid-cols-[15rem_minmax(0,1fr)] sm:grid-rows-1"
      >
        <Command.Root
          class="rounded-none border-b bg-transparent sm:border-e sm:border-b-0"
        >
          <Command.Input
            placeholder={m.task_template_search_placeholder()}
            aria-label={m.task_template_search()}
          />
          <Command.List class="max-h-none flex-1 p-1">
            <Command.Empty>{m.task_template_empty()}</Command.Empty>
            {#each templates as template (template.id)}
              <Command.Item
                value={`template-${template.id}`}
                keywords={[
                  template.name,
                  template.task.title,
                  ...template.task.labels,
                ]}
                aria-current={template.id === selected?.id ? "true" : undefined}
                class="flex-col items-stretch gap-0.5 py-2 aria-[current=true]:bg-accent aria-[current=true]:text-accent-foreground"
                onSelect={() => {
                  selected_id = template.id;
                }}
              >
                <span class="truncate text-sm font-medium">{template.name}</span
                >
                <span class="truncate text-xs text-muted-foreground">
                  {template.task.title || m.task_template_untitled()}
                </span>
              </Command.Item>
            {/each}
          </Command.List>
        </Command.Root>

        {#if selected}
          <section
            class="flex min-h-0 flex-col"
            aria-label={m.template_preview_named({ name: selected.name })}
          >
            <div class="flex items-start gap-2 px-6 pt-4 pb-3">
              <div class="min-w-0 flex-1">
                <h3 class="truncate text-base font-semibold">
                  {selected.name}
                </h3>
                <p
                  class="truncate text-sm text-muted-foreground"
                  style:color={selected.task.color
                    ? display_task_color(selected.task.color)
                    : undefined}
                >
                  {selected.task.title || m.task_template_untitled()}
                </p>
              </div>
              <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                  {#snippet child({ props })}
                    <Button
                      {...props}
                      variant="ghost"
                      size="icon-sm"
                      aria-label={m.template_actions_named({
                        name: selected.name,
                      })}
                      title={m.template_actions_named({ name: selected.name })}
                    >
                      <EllipsisIcon />
                    </Button>
                  {/snippet}
                </DropdownMenu.Trigger>
                <DropdownMenu.Content align="end">
                  {#if !read_only}
                    <DropdownMenu.Item onclick={() => onEdit(selected)}>
                      <PencilIcon />{m.ui_edit_template()}
                    </DropdownMenu.Item>
                  {/if}
                  <DropdownMenu.Item
                    onclick={() => {
                      if (board.active_board_id !== null) {
                        copying = {
                          template: selected,
                          source_id: board.active_board_id,
                        };
                      }
                    }}
                  >
                    <CopyIcon />{m.template_copy_board_title()}
                  </DropdownMenu.Item>
                  {#if !read_only}
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item
                      variant="destructive"
                      onclick={() => {
                        deleting = selected;
                      }}
                    >
                      <TrashIcon />{m.ui_delete_template()}
                    </DropdownMenu.Item>
                  {/if}
                </DropdownMenu.Content>
              </DropdownMenu.Root>
            </div>

            <ScrollArea orientation="vertical" class="min-h-0 flex-1">
              <dl class="space-y-4 px-6 pb-4 text-sm">
                {#if selected.task.priority}
                  <div class="space-y-1">
                    <dt class="text-xs font-medium text-muted-foreground">
                      {m.task_priority()}
                    </dt>
                    <dd>{priority_label(selected.task.priority)}</dd>
                  </div>
                {/if}
                {#if selected.task.labels.length > 0}
                  <div class="space-y-1">
                    <dt class="text-xs font-medium text-muted-foreground">
                      {m.task_label()}
                    </dt>
                    <dd class="flex flex-wrap gap-1">
                      {#each selected.task.labels as label (label)}
                        <LabelBadge {label} />
                      {/each}
                    </dd>
                  </div>
                {/if}
                {#if selected.task.items.length > 0}
                  <div class="space-y-1">
                    <dt class="text-xs font-medium text-muted-foreground">
                      {m.task_checklist_count({
                        count: selected.task.items.length,
                      })}
                    </dt>
                    <dd>
                      <ul class="space-y-1">
                        {#each selected.task.items as item (item.id)}
                          <li class="flex items-start gap-2">
                            <CircleIcon
                              class="mt-0.5 size-4 shrink-0 text-muted-foreground"
                            />
                            <span class="min-w-0 [overflow-wrap:anywhere]"
                              >{item.text || m.task_item()}</span
                            >
                          </li>
                        {/each}
                      </ul>
                    </dd>
                  </div>
                {/if}
                <div class="space-y-1">
                  <dt class="text-xs font-medium text-muted-foreground">
                    {m.task_description()}
                  </dt>
                  <dd>
                    {#if selected.task.description.trim()}
                      <Markdown md={selected.task.description} />
                    {:else}
                      <span class="text-muted-foreground"
                        >{m.task_description_empty()}</span
                      >
                    {/if}
                  </dd>
                </div>
              </dl>
            </ScrollArea>

            {#if !read_only}
              <form
                class="flex flex-wrap items-end gap-3 border-t px-6 py-4"
                onsubmit={(event) => {
                  event.preventDefault();
                  use_selected();
                }}
              >
                {#if columns.length === 0}
                  <p class="flex-1 text-sm text-muted-foreground">
                    {m.template_no_columns()}
                  </p>
                {:else}
                  <Field.Field class="min-w-40 flex-1">
                    <Field.FieldLabel id="template-column-label"
                      >{m.template_add_to()}</Field.FieldLabel
                    >
                    <Select.Root type="single" bind:value={column_id}>
                      <Select.Trigger
                        class="w-full"
                        aria-labelledby="template-column-label"
                        >{column_name ?? m.archive_choose()}</Select.Trigger
                      >
                      <Select.Content>
                        {#each columns as column (column.id)}
                          <Select.Item value={column.id} label={column.name}
                            >{column.name}</Select.Item
                          >
                        {/each}
                      </Select.Content>
                    </Select.Root>
                  </Field.Field>
                {/if}
                <Button type="submit" disabled={!column_id}>
                  <LayoutTemplateIcon />
                  {m.ui_use_template()}
                </Button>
              </form>
            {/if}
          </section>
        {/if}
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<AlertDialog.Root
  open={deleting !== null}
  onOpenChange={(value) => {
    if (!value) {
      deleting = null;
    }
  }}
>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{m.ui_delete_template()}</AlertDialog.Title>
      <AlertDialog.Description>
        {m.template_delete_confirm({ name: deleting?.name ?? "" })}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{m.common_cancel()}</AlertDialog.Cancel>
      <AlertDialog.Action
        variant="destructive"
        onclick={() => {
          if (deleting) {
            const id = deleting.id;
            deleting = null;
            void onDelete(id);
          }
        }}
      >
        {m.common_delete()}
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

{#if copying}
  <CopyTemplateDialog
    template={copying.template}
    source_id={copying.source_id}
    onClose={() => {
      copying = null;
    }}
  />
{/if}
