<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { tick } from "svelte";
  import type { Task } from "../../type/task.svelte";
  import type { Column } from "../../type/column.svelte";
  import TaskDialogFrame from "./task_dialog_frame.svelte";
  import TaskView from "./task_view.svelte";

  // Read-only task details, for templates and shared boards. Board tasks use
  // the task dialog, which also switches to the editor.
  let {
    open = $bindable(false),
    task,
    onViewTask,
    history,
  }: {
    open: boolean;
    task: Task | null;
    /** Opens a related task, such as one this task waits on. */
    onViewTask?: (task: Task) => void;
    /** The board holding `task`; omit for tasks without history, such as templates. */
    history?: { board_id: number; columns: Column[] };
  } = $props();

  let content_ref = $state<HTMLDivElement | null>(null);
  let view = $state<TaskView | null>(null);
  let expanded = $state(false);

  $effect(() => {
    if (!open) {
      expanded = false;
    }
  });
</script>

<TaskDialogFrame
  bind:open
  bind:content_ref
  {expanded}
  label={expanded
    ? m.layout_resize_task_description()
    : m.layout_resize_task_details()}
  onOpenFocus={() => void tick().then(() => view?.focus_initial())}
  onEscapeKeydown={(event) => {
    if (expanded) {
      event.preventDefault();
      expanded = false;
    }
  }}
>
  <TaskView
    bind:this={view}
    bind:expanded
    {task}
    dialog_ref={content_ref}
    {onViewTask}
    {history}
  />
</TaskDialogFrame>
