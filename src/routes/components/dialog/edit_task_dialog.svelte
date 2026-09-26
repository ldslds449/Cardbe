<script lang="ts">
import * as Dialog from "$lib/components/ui/dialog/index.js";
import * as Field from "$lib/components/ui/field/index.js";
import * as Popover from "$lib/components/ui/popover/index.js";
import * as ColorPicker from "$lib/components/ui/color-picker/index.js";
import * as Command from "$lib/components/ui/command/index.js";
import { TagInput } from "$lib/components/ui/tag-input/index.js";
import { Input } from "$lib/components/ui/input/index.js";
import { Button } from "$lib/components/ui/button/index.js";
import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
import { Textarea } from "$lib/components/ui/textarea/index.js";
import * as Select from "$lib/components/ui/select/index.js";
import { DragDropProvider } from "@dnd-kit-svelte/svelte";

import PaletteIcon from "@lucide/svelte/icons/palette";
import ResetIcon from "@lucide/svelte/icons/rotate-ccw";
import EyeIcon from "@lucide/svelte/icons/eye";
import PencilIcon from "@lucide/svelte/icons/pencil";
import CircleHelpIcon from "@lucide/svelte/icons/circle-help";
import CheckIcon from "@lucide/svelte/icons/check";
import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
import FilePlus2Icon from "@lucide/svelte/icons/file-plus-2";
import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
import Link2Icon from "@lucide/svelte/icons/link-2";

import { tick } from "svelte";
import { Checkbox } from "$lib/components/ui/checkbox/index.js";

import PlusIcon from "@lucide/svelte/icons/plus";
import TrashIcon from "@lucide/svelte/icons/trash-2";

import {
  type Task,
  type TaskTemplate,
  type RecurrenceFrequency,
  create_task,
  create_task_item,
} from "../../type/task.svelte";
import {
  display_task_color,
  is_preset_color,
  preset_display_color,
  task_color_presets,
} from "../../utils/task-color";
import DateTimePicker from "../date_time_picker.svelte";
import Markdown from "../markdown.svelte";
import SortableChecklistItem from "../task/sortable_checklist_item.svelte";
import { create_card_reference } from "../../utils/card-reference";

type CardReferenceOption = {
  task: Task;
  column_name: string;
};

let {
  open = $bindable(false),
  task = $bindable(create_task()),
  label_suggestions = [],
  card_reference_options = [],
  dialog_title,
  submit_button_text,
  auto_focus_title = false,
  show_template_name = false,
  template_name = $bindable(""),
  show_template_picker = false,
  templates = [],
  selected_template_id = $bindable("none"),
  on_template_change = () => {},
  dialog_done_callback = async () => {},
}: {
  open: boolean;
  task: Task;
  label_suggestions?: string[];
  card_reference_options?: CardReferenceOption[];
  dialog_title: string;
  submit_button_text: string;
  auto_focus_title?: boolean;
  show_template_name?: boolean;
  template_name?: string;
  show_template_picker?: boolean;
  templates?: TaskTemplate[];
  selected_template_id?: string;
  on_template_change?: (template_id: string) => void;
  dialog_done_callback?: () => Promise<void> | void;
} = $props();

let invalid_title = $state(false);
let invalid_template_name = $state(false);
let submitting = $state(false);
let show_saving = $state(false);
let previewing_description = $state(false);
let template_mode_active = $state(false);
let template_browser_open = $state(false);
let card_reference_picker_open = $state(false);
let dialog_content_ref = $state<HTMLDivElement | null>(null);
let title_ref = $state<HTMLInputElement | null>(null);
let description_ref = $state<HTMLTextAreaElement | null>(null);
let reference_trigger_start = $state<number | undefined>(undefined);
const selected_template = $derived(
  templates.find((template) => template.id.toString() === selected_template_id),
);

function choose_blank_card() {
  template_mode_active = false;
  template_browser_open = false;
  selected_template_id = "none";
  on_template_change(selected_template_id);
}

function choose_template(template_id: number) {
  selected_template_id = template_id.toString();
  template_browser_open = false;
  on_template_change(selected_template_id);
}

function close_template_browser() {
  template_browser_open = false;
  if (!selected_template) template_mode_active = false;
}

function open_card_reference_picker() {
  reference_trigger_start = undefined;
  card_reference_picker_open = true;
}

function handle_description_input(event: Event) {
  const textarea = event.currentTarget as HTMLTextAreaElement;
  const cursor = textarea.selectionStart;
  if (textarea.value.slice(Math.max(0, cursor - 2), cursor) === "[[") {
    reference_trigger_start = cursor - 2;
    card_reference_picker_open = true;
  }
}

async function insert_card_reference(option: CardReferenceOption) {
  const textarea = description_ref;
  const fallback_position = task.description.length;
  const selection_start = textarea?.selectionStart ?? fallback_position;
  const selection_end = textarea?.selectionEnd ?? selection_start;
  const insertion_start = reference_trigger_start ?? selection_start;
  const token = create_card_reference(option.task.title, option.task.id);
  task.description =
    task.description.slice(0, insertion_start) +
    token +
    task.description.slice(selection_end);
  card_reference_picker_open = false;
  reference_trigger_start = undefined;
  await tick();
  const cursor = insertion_start + token.length;
  description_ref?.focus();
  description_ref?.setSelectionRange(cursor, cursor);
}

function set_recurrence(value: string) {
  if (value === "none") {
    task.recurrence = undefined;
    return;
  }
  task.recurrence = {
    frequency: value as RecurrenceFrequency,
    interval: Math.max(1, task.recurrence?.interval ?? 1),
  };
}

async function focus_item(item_id: string, prevent_scroll = false) {
  await tick();
  document
    .querySelector<HTMLInputElement>(`[data-task-item-input="${item_id}"]`)
    ?.focus({ preventScroll: prevent_scroll });
}

async function add_item(after_index?: number) {
  const item = create_task_item();
  const insertion_index =
    after_index === undefined ? task.items.length : after_index + 1;
  task.items.splice(insertion_index, 0, item);
  task.items = [...task.items];
  await focus_item(item.id);
}

function remove_item(item_id: string) {
  task.items = task.items.filter((item) => item.id !== item_id);
}

async function delete_item(item_id: string) {
  const item_index = task.items.findIndex((item) => item.id === item_id);
  if (item_index === -1) return;
  const focus_target = task.items[item_index + 1] ?? task.items[item_index - 1];
  remove_item(item_id);
  if (focus_target) {
    await focus_item(focus_target.id, true);
  } else {
    await tick();
    document
      .querySelector<HTMLButtonElement>("[data-add-task-item]")
      ?.focus({ preventScroll: true });
  }
}

async function remove_blank_item(item_id: string, focus_index?: number) {
  const item = task.items.find((candidate) => candidate.id === item_id);
  if (!item || item.text.trim().length > 0) return;
  remove_item(item_id);
  const target =
    task.items[
      Math.min(focus_index ?? task.items.length - 1, task.items.length - 1)
    ];
  if (target) await focus_item(target.id);
}

function handle_item_drag_over(event: any) {
  const source = event.operation?.source;
  const target = event.operation?.target;
  if (source?.type !== "checklist-item" || target?.type !== "checklist-item")
    return;

  const source_id = source.id?.toString();
  const target_id = target.id?.toString();
  if (!source_id || !target_id || source_id === target_id) return;

  const source_index = task.items.findIndex((item) => item.id === source_id);
  const target_index = task.items.findIndex((item) => item.id === target_id);
  if (source_index === -1 || target_index === -1) return;

  const reordered_items = [...task.items];
  const [moved_item] = reordered_items.splice(source_index, 1);
  reordered_items.splice(target_index, 0, moved_item);
  task.items = reordered_items;
}

$effect(() => {
  if (!open) {
    invalid_title = false;
    invalid_template_name = false;
    previewing_description = false;
    template_mode_active = false;
    template_browser_open = false;
    card_reference_picker_open = false;
    reference_trigger_start = undefined;
  } else if (selected_template_id !== "none") {
    template_mode_active = true;
  }
});

$effect(() => {
  if (!task.due_time && task.recurrence) {
    task.recurrence = undefined;
  }
});

$effect(() => {
  if (template_browser_open) template_mode_active = true;
});

async function try_submit() {
  if (show_template_name && template_name.trim().length === 0) {
    invalid_template_name = true;
    return;
  }
  if (task.title.length === 0) {
    invalid_title = true;
    return;
  }
  if (task.id === "") {
    task.start_time = new Date();
  }
  submitting = true;
  const saving_indicator_timeout = window.setTimeout(() => {
    show_saving = true;
  }, 300);
  try {
    await dialog_done_callback();
  } finally {
    window.clearTimeout(saving_indicator_timeout);
    show_saving = false;
    submitting = false;
  }
}
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    bind:ref={dialog_content_ref}
    class="pb-0 outline-none"
    interactOutsideBehavior="ignore"
    onOpenAutoFocus={(event) => {
      event.preventDefault();
      if (auto_focus_title) {
        title_ref?.focus();
      } else {
        dialog_content_ref?.focus();
      }
    }}
  >
    <div class="flex max-h-[85vh] min-h-0 flex-col overflow-hidden">
      <Dialog.Header>
        <Dialog.Title>{dialog_title}</Dialog.Title>
        <Dialog.Description></Dialog.Description>
      </Dialog.Header>
      <form
        class="flex min-h-0 flex-1 flex-col"
        novalidate
        onsubmit={(event) => {
          event.preventDefault();
        }}
      >
        <ScrollArea orientation="vertical" class="min-h-0 flex-1 rounded-md">
          <div class="py-4 w-full max-w-md px-1">
            {#if show_template_picker && template_browser_open}
              <section class="space-y-4" aria-label="Choose a template">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  class="-ml-2"
                  onclick={close_template_browser}
                >
                  <ArrowLeftIcon />
                  Back to card
                </Button>
                <div>
                  <div class="text-base font-semibold">Choose a Template</div>
                  <div class="text-sm text-muted-foreground">
                    Search by template name, card title, or label.
                  </div>
                </div>
                <Command.Root class="rounded-lg border">
                  <Command.Input
                    placeholder="Search templates..."
                    aria-label="Search templates"
                  />
                  <Command.List class="max-h-[50vh]">
                    <Command.Empty>No templates found.</Command.Empty>
                    <Command.Group value="templates">
                      {#each templates as template (template.id)}
                        <Command.Item
                          value={template.id.toString()}
                          keywords={[
                              template.name,
                              template.task.title,
                              ...template.task.labels,
                            ]}
                          class="py-3"
                          onSelect={() => choose_template(template.id)}
                        >
                          <CheckIcon
                            class={selected_template_id === template.id.toString()
                                ? ""
                                : "text-transparent"}
                          />
                          <div class="min-w-0 flex-1">
                            <div class="truncate text-sm font-medium">
                              {template.name}
                            </div>
                            <div class="truncate text-xs text-muted-foreground">
                              {template.task.title || "Untitled card"}
                              ·
                              {template.task.items.length}
                              {template.task.items.length === 1
                                  ? " checklist item"
                                  : " checklist items"}
                            </div>
                          </div>
                        </Command.Item>
                      {/each}
                    </Command.Group>
                  </Command.List>
                </Command.Root>
              </section>
            {:else}
              {#if show_template_picker && templates.length > 0}
                <section
                  class="mb-5 space-y-3 border-b pb-5"
                  aria-label="Creation method"
                >
                  <div
                    class="grid grid-cols-2 gap-2 rounded-lg bg-muted/60 p-1"
                  >
                    <Button
                      type="button"
                      variant={!template_mode_active ? "default" : "ghost"}
                      class="justify-start"
                      onclick={choose_blank_card}
                    >
                      <FilePlus2Icon />
                      Blank Card
                    </Button>
                    <Button
                      type="button"
                      variant={template_mode_active ? "default" : "ghost"}
                      class="justify-start"
                      onclick={() => {
                        template_mode_active = true;
                        template_browser_open = true;
                      }}
                    >
                      <LayoutTemplateIcon />
                      From Template
                    </Button>
                  </div>
                  {#if template_mode_active}
                    {#if selected_template}
                      <div
                        class="flex items-center gap-3 rounded-lg border border-primary/40 bg-primary/5 p-3"
                      >
                        <div
                          class="flex size-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary"
                        >
                          <LayoutTemplateIcon class="size-4" />
                        </div>
                        <div class="min-w-0 flex-1">
                          <div class="truncate text-sm font-medium">
                            {selected_template.name}
                          </div>
                          <div class="truncate text-xs text-muted-foreground">
                            {selected_template.task.title || "Untitled card"}
                            ·
                            {selected_template.task.items.length}
                            {selected_template.task.items.length === 1
                                ? " checklist item"
                                : " checklist items"}
                          </div>
                        </div>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          onclick={() => {
                              template_browser_open = true;
                            }}
                        >
                          Change
                        </Button>
                      </div>
                    {/if}
                  {/if}
                </section>
              {/if}
              <Field.Set>
                <Field.Group>
                  {#if show_template_name}
                    <Field.Field data-invalid={invalid_template_name}>
                      <Field.FieldLabel for="template-edit-name"
                        >Template name</Field.FieldLabel
                      >
                      <Input
                        id="template-edit-name"
                        placeholder="Template name"
                        bind:value={template_name}
                        required
                        onfocusout={() => {
                          invalid_template_name = template_name.trim().length === 0;
                        }}
                        onfocusin={() => {
                          invalid_template_name = false;
                        }}
                      />
                      {#if invalid_template_name}
                        <Field.Error
                          >Please enter the template name</Field.Error
                        >
                      {/if}
                    </Field.Field>
                  {/if}
                  <Field.Field data-invalid={invalid_title}>
                    <Field.FieldLabel for="title">Title</Field.FieldLabel>
                    <Input
                      id="title"
                      bind:ref={title_ref}
                      placeholder="Title"
                      class="col-span-5"
                      bind:value={task.title}
                      required
                      onfocusout={() => {
                        invalid_title = task.title.length == 0;
                      }}
                      onfocusin={() => {
                        invalid_title = false;
                      }}
                    />
                    {#if invalid_title}
                      <Field.Error>Please enter the title</Field.Error>
                    {/if}
                  </Field.Field>
                  <Field.Field>
                    <Popover.Root>
                      <Popover.Trigger>
                        {#snippet child({ props })}
                          <div class="flex items-center gap-4">
                            <div
                              class="size-8 rounded-full border-2"
                              style="background-color: {task.color.length == 0
                                ? 'var(--foreground)'
                                : display_task_color(task.color)};"
                            ></div>
                            <Button {...props} variant="outline" class="flex-1">
                              <PaletteIcon></PaletteIcon>
                              {task.color.length == 0
                                ? "Default"
                                : display_task_color(task.color)}
                            </Button>
                            <Button
                              variant="outline"
                              aria-label="Reset task color"
                              title="Reset task color"
                              onclick={() => {
                                task.color = "";
                              }}
                            >
                              <ResetIcon></ResetIcon>
                            </Button>
                          </div>
                        {/snippet}
                      </Popover.Trigger>
                      <Popover.Content class="w-auto p-0">
                        <div class="border-b p-3">
                          <p
                            class="mb-2 text-xs font-medium text-muted-foreground"
                          >
                            Quick colors
                          </p>
                          <div class="grid grid-cols-9 gap-2">
                            {#each task_color_presets as preset}
                              <button
                                type="button"
                                class="size-7 rounded-full border-2 border-background shadow-sm ring-offset-2 ring-offset-background transition-transform hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                class:ring-2={is_preset_color(task.color, preset)}
                                class:ring-ring={is_preset_color(task.color, preset)}
                                style:background-color={preset_display_color(preset)}
                                aria-label={`Use ${preset.name} (${preset_display_color(preset)})`}
                                aria-pressed={is_preset_color(task.color, preset)}
                                title={preset.name}
                                onclick={() => {
                                  task.color = preset.light;
                                }}
                              ></button>
                            {/each}
                          </div>
                        </div>
                        <div class="p-3">
                          <ColorPicker.Root bind:value={task.color} />
                        </div>
                      </Popover.Content>
                    </Popover.Root>
                  </Field.Field>
                  <Field.Field>
                    <DateTimePicker bind:value={task.due_time} />
                  </Field.Field>
                  {#if task.due_time}
                    <Field.Field>
                      <Field.FieldLabel>Repeat</Field.FieldLabel>
                      <div class="flex items-center gap-2">
                        {#if task.recurrence}
                          <span class="text-sm text-muted-foreground"
                            >Every</span
                          >
                          <Input
                            class="w-20"
                            type="number"
                            min="1"
                            max="999"
                            aria-label="Repeat interval"
                            bind:value={task.recurrence.interval}
                            onfocusout={() => {
                              if (task.recurrence) {
                                task.recurrence.interval = Math.min(
                                  999,
                                  Math.max(1, Number(task.recurrence.interval) || 1),
                                );
                              }
                            }}
                          />
                        {/if}
                        <Select.Root
                          type="single"
                          value={task.recurrence?.frequency ?? "none"}
                          onValueChange={(value) => set_recurrence(value ?? "none")}
                        >
                          <Select.Trigger class="min-w-40 flex-1">
                            {task.recurrence?.frequency === "daily"
                              ? "day(s)"
                              : task.recurrence?.frequency === "weekly"
                                ? "week(s)"
                                : task.recurrence?.frequency === "monthly"
                                  ? "month(s)"
                                  : "Does not repeat"}
                          </Select.Trigger>
                          <Select.Content>
                            <Select.Item value="none"
                              >Does not repeat</Select.Item
                            >
                            <Select.Item value="daily">Day(s)</Select.Item>
                            <Select.Item value="weekly">Week(s)</Select.Item>
                            <Select.Item value="monthly">Month(s)</Select.Item>
                          </Select.Content>
                        </Select.Root>
                      </div>
                      {#if task.recurrence}
                        <Field.Description>
                          Archiving this task creates the next occurrence.
                        </Field.Description>
                      {/if}
                    </Field.Field>
                  {/if}
                  <Field.Field>
                    <div class="flex items-center gap-1.5">
                      <Field.FieldLabel for="label">Label</Field.FieldLabel>
                      <div class="group/label-help relative inline-flex">
                        <button
                          type="button"
                          class="rounded-full text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                          aria-label="Label format help"
                          aria-describedby="label-format-help"
                        >
                          <CircleHelpIcon class="size-3.5" />
                        </button>
                        <div
                          id="label-format-help"
                          role="tooltip"
                          class="pointer-events-none absolute bottom-full left-0 z-50 mb-2 w-max max-w-64 rounded-md bg-popover px-3 py-2 text-xs leading-relaxed text-popover-foreground opacity-0 shadow-md ring-1 ring-foreground/10 transition-opacity group-hover/label-help:opacity-100 group-focus-within/label-help:opacity-100"
                        >
                          Use <code>owner:Name</code> for an owner.<br>
                          Use <code>type:Category</code> for a task category.<br>
                          Use <code>priority:Level</code> for priority.<br>
                          Use <code>status:State</code> for status.<br>
                          Use <code>effort:Size</code> for effort.
                        </div>
                      </div>
                    </div>
                    <TagInput
                      bind:tags={task.labels}
                      suggestions={label_suggestions}
                      placeholder="Add Label"
                    />
                  </Field.Field>
                  <Field.Field>
                    <Field.FieldLabel>Checklist</Field.FieldLabel>
                    <div class="space-y-2">
                      <DragDropProvider onDragOver={handle_item_drag_over}>
                        <div role="list" class="space-y-2">
                          {#each task.items as item, item_index (item.id)}
                            <SortableChecklistItem
                              item_id={item.id}
                              index={item_index}
                            >
                              <Checkbox
                                bind:checked={item.completed}
                                aria-label={`Mark ${item.text || "item"} as ${item.completed ? "not completed" : "completed"}`}
                              />
                              <Input
                                class={`h-7 min-w-0 flex-1 border-0 px-1.5 py-1 text-sm shadow-none focus-visible:ring-0 ${item.completed ? "text-muted-foreground line-through" : ""}`}
                                placeholder="Item"
                                data-task-item-input={item.id}
                                bind:value={item.text}
                                onfocusout={() => void remove_blank_item(item.id)}
                                onkeydown={(event) => {
                              if (event.key === "Enter") {
                                event.preventDefault();
                                void add_item(item_index);
                              } else if (event.key === "Escape" && item.text.trim().length === 0) {
                                event.preventDefault();
                                void remove_blank_item(item.id, item_index - 1);
                              }
                            }}
                              />
                              <Button
                                variant="ghost"
                                size="icon-sm"
                                class="size-7 text-muted-foreground hover:text-destructive"
                                aria-label="Delete item"
                                title="Delete item"
                                onpointerdown={(event) => event.preventDefault()}
                                onclick={() => void delete_item(item.id)}
                              >
                                <TrashIcon />
                              </Button>
                            </SortableChecklistItem>
                          {/each}
                        </div>
                      </DragDropProvider>
                      <Button
                        data-add-task-item
                        variant="outline"
                        size="sm"
                        class="h-8 w-full"
                        onclick={() => void add_item()}
                      >
                        <PlusIcon />
                        Add item
                      </Button>
                    </div>
                  </Field.Field>
                  <Field.Field>
                    <div class="flex items-center justify-between gap-2">
                      <div class="flex items-center gap-1.5">
                        <Field.FieldLabel for="description"
                          >Description</Field.FieldLabel
                        >
                        <div class="group/reference-help relative inline-flex">
                          <button
                            type="button"
                            class="rounded-full text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                            aria-label="Card reference help"
                            aria-describedby="card-reference-help"
                          >
                            <CircleHelpIcon class="size-3.5" />
                          </button>
                          <div
                            id="card-reference-help"
                            role="tooltip"
                            class="pointer-events-none absolute bottom-full left-0 z-50 mb-2 w-max max-w-64 rounded-md bg-popover px-3 py-2 text-xs leading-relaxed text-popover-foreground opacity-0 shadow-md ring-1 ring-foreground/10 transition-opacity group-hover/reference-help:opacity-100 group-focus-within/reference-help:opacity-100"
                          >
                            Type <code>[[</code> to search for and reference
                            another card.
                          </div>
                        </div>
                      </div>
                      <div class="flex items-center gap-1">
                        {#if !previewing_description && card_reference_options.length > 0}
                          <Popover.Root bind:open={card_reference_picker_open}>
                            <Popover.Trigger>
                              {#snippet child({ props })}
                                <Button
                                  {...props}
                                  type="button"
                                  variant="ghost"
                                  size="sm"
                                  class="h-7 select-none gap-1.5 px-2 text-xs"
                                  onclick={open_card_reference_picker}
                                >
                                  <Link2Icon class="size-3.5" />
                                  Reference card
                                </Button>
                              {/snippet}
                            </Popover.Trigger>
                            <Popover.Content class="w-80 p-0" align="end">
                              <Command.Root>
                                <Command.Input
                                  placeholder="Search cards..."
                                  aria-label="Search cards"
                                />
                                <Command.List class="max-h-64">
                                  <Command.Empty>No cards found.</Command.Empty>
                                  <Command.Group value="cards">
                                    {#each card_reference_options.filter((option) => option.task.id !== task.id) as option (option.task.id)}
                                      <Command.Item
                                        value={option.task.id}
                                        keywords={[option.task.title, option.column_name]}
                                        onSelect={() => void insert_card_reference(option)}
                                      >
                                        <div class="min-w-0">
                                          <div class="truncate">
                                            {option.task.title}
                                          </div>
                                          <div
                                            class="truncate text-xs text-muted-foreground"
                                          >
                                            {option.column_name}
                                          </div>
                                        </div>
                                      </Command.Item>
                                    {/each}
                                  </Command.Group>
                                </Command.List>
                              </Command.Root>
                            </Popover.Content>
                          </Popover.Root>
                        {/if}
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          class="h-7 w-20 select-none justify-center gap-1.5 px-2 text-xs"
                          aria-pressed={previewing_description}
                          onclick={() => {
                            previewing_description = !previewing_description;
                          }}
                        >
                          {#if previewing_description}
                            <PencilIcon class="size-3.5" />
                            Edit
                          {:else}
                            <EyeIcon class="size-3.5" />
                            Preview
                          {/if}
                        </Button>
                      </div>
                    </div>
                    <div class="grid w-full gap-4">
                      {#if previewing_description}
                        <div
                          class="h-80 overflow-y-auto rounded-md border bg-muted/30 px-3 py-2 text-sm"
                          aria-label="Description preview"
                        >
                          {#if task.description.trim().length > 0}
                            <Markdown md={task.description} />
                          {:else}
                            <p class="text-muted-foreground">
                              Nothing to preview
                            </p>
                          {/if}
                        </div>
                      {:else}
                        <Textarea
                          bind:ref={description_ref}
                          class="h-80 resize-y"
                          placeholder="Description (Markdown supported)"
                          id="description"
                          bind:value={task.description}
                          oninput={handle_description_input}
                        />
                      {/if}
                    </div>
                  </Field.Field>
                </Field.Group>
              </Field.Set>
            {/if}
          </div>
        </ScrollArea>
        {#if !template_browser_open}
          <Dialog.Footer
            class="mt-auto shrink-0 justify-center border-t bg-background/95 px-1 py-3 shadow-[0_-8px_16px_-16px_rgb(0_0_0_/_0.45)] backdrop-blur-sm sm:justify-center"
          >
            <Button
              class="min-w-40"
              disabled={task.title.length == 0 ||
              (show_template_name && !template_name.trim()) ||
              (show_template_picker && template_mode_active && selected_template_id === "none") ||
              submitting}
              type="button"
              onclick={() => void try_submit()}
              >{show_saving ? "Saving..." : submit_button_text}</Button
            >
          </Dialog.Footer>
        {/if}
      </form>
    </div>
  </Dialog.Content>
</Dialog.Root>
