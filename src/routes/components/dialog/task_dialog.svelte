<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { tick } from "svelte";
  import * as Tabs from "#lib/components/ui/tabs/index.js";
  import { confirmUnsavedChanges } from "#lib/unsaved-changes.js";
  import EyeIcon from "@lucide/svelte/icons/eye";
  import PencilIcon from "@lucide/svelte/icons/pencil";

  import {
    create_task,
    type Task,
    type TaskTemplate,
  } from "../../type/task.svelte";
  import type { Column } from "../../type/column.svelte";
  import TaskDialogFrame from "./task_dialog_frame.svelte";
  import TaskView from "./task_view.svelte";
  import TaskEditor from "./task_editor.svelte";

  type CardReferenceOption = {
    task: Task;
    column_name: string;
  };

  // One dialog for viewing and editing a task, so switching modes or
  // expanding the description changes its content in place.
  let {
    open = $bindable(false),
    mode,
    view_task,
    onEdit,
    onView,
    onViewTask,
    history,
    task = $bindable(create_task()),
    label_suggestions = [],
    card_reference_options = [],
    columns = [],
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
    mode: "view" | "edit";
    /** The task shown in view mode. */
    view_task: Task | null;
    /** Switches to editing `view_task`; omit when it is read-only. */
    onEdit?: () => void;
    /** Switches back to viewing; omit for drafts without saved details. */
    onView?: () => void;
    /** Opens a related task, such as one this task waits on. */
    onViewTask?: (task: Task) => void;
    /** The board holding `view_task`. */
    history?: { board_id: number; columns: Column[] };
    /** The draft edited in edit mode. */
    task: Task;
    label_suggestions?: string[];
    card_reference_options?: CardReferenceOption[];
    columns?: Column[];
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

  let content_ref = $state<HTMLDivElement | null>(null);
  let view = $state<TaskView | null>(null);
  let editor = $state<TaskEditor | null>(null);
  let expanded = $state(false);
  let submitting = $state(false);

  $effect(() => {
    if (!open) {
      expanded = false;
    }
  });

  // An open dialog that changes mode focuses the new content; opening the
  // dialog focuses it through the frame instead.
  let previous_mode: "view" | "edit" | undefined;
  let was_open = false;
  $effect(() => {
    const current = mode;
    const is_open = open;
    if (is_open && was_open && current !== previous_mode) {
      void focus_content();
    }
    previous_mode = current;
    was_open = is_open;
  });

  // Waits for the content to mount and bind before focusing it.
  async function focus_content() {
    await tick();
    if (mode === "edit") {
      void editor?.focus_initial();
    } else {
      view?.focus_initial();
    }
  }

  async function switch_mode(next: string) {
    if (next === mode) {
      return;
    }
    if (next === "edit") {
      onEdit?.();
    } else if (await confirmUnsavedChanges()) {
      onView?.();
    }
  }

  const resize_label = $derived(
    expanded
      ? m.layout_resize_task_description()
      : mode === "edit"
        ? m.layout_resize_task_editor()
        : m.layout_resize_task_details(),
  );
</script>

{#snippet mode_switch()}
  {#if onEdit && onView}
    <Tabs.Root bind:value={() => mode, (next) => switch_mode(next)}>
      <Tabs.List aria-label={m.task_dialog_mode()}>
        <Tabs.Trigger value="view">
          <EyeIcon />
          <span class="hidden sm:inline">{m.task_mode_view()}</span>
        </Tabs.Trigger>
        <Tabs.Trigger value="edit">
          <PencilIcon />
          <span class="hidden sm:inline">{m.common_edit()}</span>
        </Tabs.Trigger>
      </Tabs.List>
    </Tabs.Root>
  {/if}
{/snippet}

<TaskDialogFrame
  bind:open
  bind:content_ref
  {expanded}
  label={resize_label}
  draft={mode === "edit" ? { task, template_name } : undefined}
  draftVersion={mode === "edit" ? 1 : 0}
  busy={mode === "edit" && submitting}
  onOpenFocus={focus_content}
  onkeydown={(event) => {
    if (mode === "edit" && event.target === content_ref) {
      editor?.handle_submit_shortcut(event);
    }
  }}
  onEscapeKeydown={(event) => {
    if (expanded) {
      event.preventDefault();
      if (mode === "edit") {
        editor?.collapse_description();
      } else {
        expanded = false;
      }
    } else if (mode === "edit") {
      editor?.commit_pending();
    }
  }}
  onInteractOutside={() => {
    if (mode === "edit") {
      editor?.commit_pending();
    }
  }}
>
  {#if mode === "edit"}
    <TaskEditor
      bind:this={editor}
      bind:task
      bind:template_name
      bind:selected_template_id
      bind:expanded
      bind:submitting
      dialog_ref={content_ref}
      header_actions={mode_switch}
      {label_suggestions}
      {card_reference_options}
      {columns}
      {dialog_title}
      {submit_button_text}
      {auto_focus_title}
      {show_template_name}
      {show_template_picker}
      {templates}
      {on_template_change}
      {dialog_done_callback}
    />
  {:else}
    <TaskView
      bind:this={view}
      bind:expanded
      task={view_task}
      dialog_ref={content_ref}
      header_actions={mode_switch}
      {onEdit}
      {onViewTask}
      {history}
    />
  {/if}
</TaskDialogFrame>
