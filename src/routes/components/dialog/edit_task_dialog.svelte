<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import * as ColorPicker from "$lib/components/ui/color-picker/index.js";
  import * as Command from "$lib/components/ui/command/index.js";
  import { TagInput } from "$lib/components/ui/tag-input/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import {
    RichTextEditor,
    type RichTextEditorHandle,
  } from "$lib/components/editor/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { DragDropProvider } from "@dnd-kit-svelte/svelte";

  import PaletteIcon from "@lucide/svelte/icons/palette";
  import ResetIcon from "@lucide/svelte/icons/rotate-ccw";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import Minimize2Icon from "@lucide/svelte/icons/minimize-2";
  import CircleHelpIcon from "@lucide/svelte/icons/circle-help";
  import CheckIcon from "@lucide/svelte/icons/check";
  import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
  import FilePlus2Icon from "@lucide/svelte/icons/file-plus-2";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import CardReferencePicker from "$lib/components/editor/card-reference-picker.svelte";

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
  import SortableChecklistItem from "../task/sortable_checklist_item.svelte";

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
  let description_expanded = $state(false);
  let template_mode_active = $state(false);
  let template_browser_open = $state(false);
  let card_reference_picker_open = $state(false);
  let dialog_content_ref = $state<HTMLDivElement | null>(null);
  let title_ref = $state<HTMLInputElement | null>(null);
  let description_ref = $state<RichTextEditorHandle | null>(null);
  let expanded_description_ref = $state<RichTextEditorHandle | null>(null);
  let reference_trigger_start = $state<number | undefined>(undefined);
  const selected_template = $derived(
    templates.find(
      (template) => template.id.toString() === selected_template_id,
    ),
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
    if (!selected_template) {
      template_mode_active = false;
    }
  }

  function handle_description_change() {
    const editor_ref = description_expanded
      ? expanded_description_ref
      : description_ref;
    const selection = editor_ref?.getSelection();
    if (!selection) {
      return;
    }
    if (editor_ref?.getTextBeforeCursor(2) === "[[") {
      reference_trigger_start = selection.from - 2;
      card_reference_picker_open = true;
    }
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
    if (item_index === -1) {
      return;
    }
    const focus_target =
      task.items[item_index + 1] ?? task.items[item_index - 1];
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
    if (!item || item.text.trim().length > 0) {
      return;
    }
    remove_item(item_id);
    const target =
      task.items[
        Math.min(focus_index ?? task.items.length - 1, task.items.length - 1)
      ];
    if (target) {
      await focus_item(target.id);
    }
  }

  function handle_item_drag_over(event: any) {
    const source = event.operation?.source;
    const target = event.operation?.target;
    if (
      source?.type !== "checklist-item" ||
      target?.type !== "checklist-item"
    ) {
      return;
    }

    const source_id = source.id?.toString();
    const target_id = target.id?.toString();
    if (!source_id || !target_id || source_id === target_id) {
      return;
    }

    const source_index = task.items.findIndex((item) => item.id === source_id);
    const target_index = task.items.findIndex((item) => item.id === target_id);
    if (source_index === -1 || target_index === -1) {
      return;
    }

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
      description_expanded = false;
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
    if (template_browser_open) {
      template_mode_active = true;
    }
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

{#snippet card_reference_picker()}
  <CardReferencePicker
    editor={description_expanded ? expanded_description_ref : description_ref}
    options={card_reference_options}
    excludeTaskId={task.id}
    bind:open={card_reference_picker_open}
    bind:triggerStart={reference_trigger_start}
  />
{/snippet}

<Dialog.Root bind:open draft={{ task, template_name }} busy={submitting}>
  <Dialog.Content
    bind:ref={dialog_content_ref}
    class="pb-0 outline-none"
    interactOutsideBehavior="close"
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
              <section class="space-y-4" aria-label={m.task_template_choose()}>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  class="-ml-2"
                  onclick={close_template_browser}
                >
                  <ArrowLeftIcon />
                  {m.task_template_back()}
                </Button>
                <div>
                  <div class="text-base font-semibold">
                    {m.task_template_choose_title()}
                  </div>
                  <div class="text-sm text-muted-foreground">
                    {m.task_template_search_description()}
                  </div>
                </div>
                <Command.Root class="rounded-lg border">
                  <Command.Input
                    placeholder={m.task_template_search_placeholder()}
                    aria-label={m.task_template_search()}
                  />
                  <Command.List class="max-h-[50vh]">
                    <Command.Empty>{m.task_template_empty()}</Command.Empty>
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
                            class={selected_template_id ===
                            template.id.toString()
                              ? ""
                              : "text-transparent"}
                          />
                          <div class="min-w-0 flex-1">
                            <div class="truncate text-sm font-medium">
                              {template.name}
                            </div>
                            <div class="truncate text-xs text-muted-foreground">
                              {template.task.title ||
                                m.task_template_untitled()}
                              ·
                              {m.task_checklist_count({
                                count: template.task.items.length,
                              })}
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
                  aria-label={m.task_template_method()}
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
                      {m.task_template_blank_title()}
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
                      {m.task_template_from()}
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
                            {selected_template.task.title ||
                              m.task_template_untitled()}
                            ·
                            {m.task_checklist_count({
                              count: selected_template.task.items.length,
                            })}
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
                          {m.common_change()}
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
                        >{m.task_template_name()}</Field.FieldLabel
                      >
                      <Input
                        id="template-edit-name"
                        placeholder={m.task_template_name()}
                        bind:value={template_name}
                        required
                        onfocusout={() => {
                          invalid_template_name =
                            template_name.trim().length === 0;
                        }}
                        onfocusin={() => {
                          invalid_template_name = false;
                        }}
                      />
                      {#if invalid_template_name}
                        <Field.Error
                          >{m.task_template_name_required()}</Field.Error
                        >
                      {/if}
                    </Field.Field>
                  {/if}
                  <Field.Field data-invalid={invalid_title}>
                    <Field.FieldLabel for="title"
                      >{m.task_title()}</Field.FieldLabel
                    >
                    <Input
                      id="title"
                      bind:ref={title_ref}
                      placeholder={m.task_title()}
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
                      <Field.Error>{m.task_title_required()}</Field.Error>
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
                                ? m.ui_default()
                                : display_task_color(task.color)}
                            </Button>
                            <Button
                              variant="outline"
                              aria-label={m.task_color_reset()}
                              title={m.task_color_reset()}
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
                            {m.task_quick_colors()}
                          </p>
                          <div class="grid grid-cols-9 gap-2">
                            {#each task_color_presets as preset}
                              <Button
                                type="button"
                                variant="outline"
                                size="icon-xs"
                                class="size-7 rounded-full border-2 border-background shadow-sm ring-offset-2 ring-offset-background aria-pressed:ring-2 aria-pressed:ring-ring"
                                style={`background-color: ${preset_display_color(preset)}`}
                                aria-label={m.task_color_use({
                                  name: {
                                    Slate: m.color_slate,
                                    Red: m.color_red,
                                    Orange: m.color_orange,
                                    Amber: m.color_amber,
                                    Green: m.color_green,
                                    Cyan: m.color_cyan,
                                    Blue: m.color_blue,
                                    Violet: m.color_violet,
                                    Pink: m.color_pink,
                                  }[preset.name](),
                                  color: preset_display_color(preset),
                                })}
                                aria-pressed={is_preset_color(
                                  task.color,
                                  preset,
                                )}
                                title={preset.name}
                                onclick={() => {
                                  task.color = preset.light;
                                }}
                              ></Button>
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
                      <Field.FieldLabel>{m.task_repeat()}</Field.FieldLabel>
                      <div class="flex items-center gap-2">
                        {#if task.recurrence}
                          <span class="text-sm text-muted-foreground"
                            >{m.task_repeat_every()}</span
                          >
                          <Input
                            class="w-20"
                            type="number"
                            min="1"
                            max="999"
                            aria-label={m.task_repeat_interval()}
                            bind:value={task.recurrence.interval}
                            onfocusout={() => {
                              if (task.recurrence) {
                                task.recurrence.interval = Math.min(
                                  999,
                                  Math.max(
                                    1,
                                    Number(task.recurrence.interval) || 1,
                                  ),
                                );
                              }
                            }}
                          />
                        {/if}
                        <Select.Root
                          type="single"
                          value={task.recurrence?.frequency ?? "none"}
                          onValueChange={(value) =>
                            set_recurrence(value ?? "none")}
                        >
                          <Select.Trigger class="min-w-40 flex-1">
                            {task.recurrence?.frequency === "daily"
                              ? m.task_repeat_days_label()
                              : task.recurrence?.frequency === "weekly"
                                ? m.task_repeat_weeks_label()
                                : task.recurrence?.frequency === "monthly"
                                  ? m.task_repeat_months_label()
                                  : m.task_repeat_none()}
                          </Select.Trigger>
                          <Select.Content>
                            <Select.Item value="none"
                              >{m.task_repeat_none()}</Select.Item
                            >
                            <Select.Item value="daily"
                              >{m.task_repeat_days()}</Select.Item
                            >
                            <Select.Item value="weekly"
                              >{m.task_repeat_weeks()}</Select.Item
                            >
                            <Select.Item value="monthly"
                              >{m.task_repeat_months()}</Select.Item
                            >
                          </Select.Content>
                        </Select.Root>
                      </div>
                      {#if task.recurrence}
                        <Field.Description>
                          {m.task_repeat_description()}
                        </Field.Description>
                      {/if}
                    </Field.Field>
                  {/if}
                  <Field.Field>
                    <div class="flex items-center gap-1.5">
                      <Field.FieldLabel for="label"
                        >{m.task_label()}</Field.FieldLabel
                      >
                      <div class="group/label-help relative inline-flex">
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-xs"
                          class="text-muted-foreground"
                          aria-label={m.task_label_help()}
                          aria-describedby="label-format-help"
                        >
                          <CircleHelpIcon class="size-3.5" />
                        </Button>
                        <div
                          id="label-format-help"
                          role="tooltip"
                          class="pointer-events-none absolute bottom-full left-0 z-50 mb-2 w-max max-w-64 rounded-md bg-popover px-3 py-2 text-xs leading-relaxed text-popover-foreground opacity-0 shadow-md ring-1 ring-foreground/10 transition-opacity group-hover/label-help:opacity-100 group-focus-within/label-help:opacity-100"
                        >
                          {m.task_label_owner_help({
                            example: "owner:Name",
                          })}<br />
                          {m.task_label_type_help({
                            example: "type:Category",
                          })}<br />
                          {m.task_label_priority_help({
                            example: "priority:Level",
                          })}<br />
                          {m.task_label_status_help({
                            example: "status:State",
                          })}<br />
                          {m.task_label_effort_help({ example: "effort:Size" })}
                        </div>
                      </div>
                    </div>
                    <TagInput
                      bind:tags={task.labels}
                      suggestions={label_suggestions}
                      placeholder={m.task_label_add()}
                    />
                  </Field.Field>
                  <Field.Field>
                    <Field.FieldLabel>{m.task_checklist()}</Field.FieldLabel>
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
                                aria-label={item.completed
                                  ? m.task_item_mark_incomplete({
                                      name: item.text || m.task_item(),
                                    })
                                  : m.task_item_mark_complete({
                                      name: item.text || m.task_item(),
                                    })}
                              />
                              <Input
                                class={`h-7 min-w-0 flex-1 border-0 px-1.5 py-1 text-sm shadow-none focus-visible:ring-0 ${item.completed ? "text-muted-foreground line-through" : ""}`}
                                placeholder={m.task_item()}
                                data-task-item-input={item.id}
                                bind:value={item.text}
                                onfocusout={() =>
                                  void remove_blank_item(item.id)}
                                onkeydown={(event) => {
                                  if (event.key === "Enter") {
                                    event.preventDefault();
                                    void add_item(item_index);
                                  } else if (
                                    event.key === "Escape" &&
                                    item.text.trim().length === 0
                                  ) {
                                    event.preventDefault();
                                    void remove_blank_item(
                                      item.id,
                                      item_index - 1,
                                    );
                                  }
                                }}
                              />
                              <Button
                                variant="ghost"
                                size="icon-sm"
                                class="size-7 text-muted-foreground hover:text-destructive"
                                aria-label={m.task_item_delete()}
                                title={m.task_item_delete()}
                                onpointerdown={(event) =>
                                  event.preventDefault()}
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
                        {m.task_item_add()}
                      </Button>
                    </div>
                  </Field.Field>
                  <Field.Field>
                    <div class="flex items-center justify-between gap-2">
                      <div class="flex items-center gap-1.5">
                        <Field.FieldLabel for="description"
                          >{m.task_description()}</Field.FieldLabel
                        >
                        <div class="group/reference-help relative inline-flex">
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            class="text-muted-foreground"
                            aria-label={m.task_reference_help()}
                            aria-describedby="card-reference-help"
                          >
                            <CircleHelpIcon class="size-3.5" />
                          </Button>
                          <div
                            id="card-reference-help"
                            role="tooltip"
                            class="pointer-events-none absolute bottom-full left-0 z-50 mb-2 w-max max-w-64 rounded-md bg-popover px-3 py-2 text-xs leading-relaxed text-popover-foreground opacity-0 shadow-md ring-1 ring-foreground/10 transition-opacity group-hover/reference-help:opacity-100 group-focus-within/reference-help:opacity-100"
                          >
                            {m.task_reference_hint()}
                          </div>
                        </div>
                      </div>
                      <div class="flex items-center gap-1">
                        {#if !description_expanded && !previewing_description && card_reference_options.length > 0}
                          {@render card_reference_picker()}
                        {/if}
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          class="h-7 gap-1.5 px-2 text-xs"
                          aria-label={m.task_expand_description()}
                          title={m.task_expand_description()}
                          onclick={() => (description_expanded = true)}
                        >
                          <Maximize2Icon class="size-3.5" />
                          <span class="hidden sm:inline"
                            >{m.editor_expand()}</span
                          >
                        </Button>
                      </div>
                    </div>
                    <div class="grid w-full gap-4">
                      <RichTextEditor
                        bind:previewing={previewing_description}
                        bind:ref={description_ref}
                        containerClass="rounded-md border bg-background"
                        class="h-80 overflow-y-auto px-3 py-2"
                        placeholder={m.task_description_placeholder()}
                        id="description"
                        bind:value={task.description}
                        onvaluechange={handle_description_change}
                      />
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
                (show_template_picker &&
                  template_mode_active &&
                  selected_template_id === "none") ||
                submitting}
              type="button"
              onclick={() => void try_submit()}
              >{show_saving ? m.common_saving() : submit_button_text}</Button
            >
          </Dialog.Footer>
        {/if}
      </form>
    </div>
  </Dialog.Content>
  <Dialog.Root bind:open={description_expanded}>
    <Dialog.Content
      class="flex h-[min(90vh,56rem)] w-[calc(100vw-1rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl"
      showCloseButton={false}
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        void tick().then(() => expanded_description_ref?.focus());
      }}
    >
      <Dialog.Header class="shrink-0 border-b px-4 py-3 text-start sm:px-6">
        <div class="flex items-center justify-between gap-3">
          <Dialog.Title class="min-w-0 flex-1"
            >{m.task_description()}</Dialog.Title
          >
          {#if !previewing_description && card_reference_options.length > 0}
            {@render card_reference_picker()}
          {/if}
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Collapse description editor"
            title="Collapse description editor"
            onclick={() => (description_expanded = false)}
          >
            <Minimize2Icon />
          </Button>
        </div>
        <Dialog.Description class="sr-only"
          >{m.editor_expanded_description()}</Dialog.Description
        >
      </Dialog.Header>
      <div class="min-h-0 flex-1 p-3 sm:p-5">
        <RichTextEditor
          bind:ref={expanded_description_ref}
          containerClass="flex h-full min-h-0 flex-col overflow-hidden rounded-md border bg-background"
          class="min-h-0 flex-1 overflow-y-auto px-3 py-4 sm:px-6"
          placeholder={m.task_description_placeholder()}
          showToolbar
          value={task.description}
          onvaluechange={(content) => {
            task.description = content;
            handle_description_change();
          }}
        />
      </div>
    </Dialog.Content>
  </Dialog.Root>
</Dialog.Root>
