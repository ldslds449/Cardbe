<script lang="ts">
  import TaskActionItems from "../task/task_action_items.svelte";
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Collapsible from "$lib/components/ui/collapsible/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { useSortable } from "@dnd-kit-svelte/svelte/sortable";

  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import Share2Icon from "@lucide/svelte/icons/share-2";
  import PinIcon from "@lucide/svelte/icons/pin";
  import PictureInPicture2Icon from "@lucide/svelte/icons/picture-in-picture-2";
  import MoveRightIcon from "@lucide/svelte/icons/move-right";
  import MoveTaskDialog from "../dialog/move_task_dialog.svelte";

  import { recurrence_label, type Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";
  import Markdown from "../markdown.svelte";
  import LabelBadge from "../label_badge.svelte";
  import ChecklistDisplay from "./checklist_display.svelte";
  import DeleteTaskDialog from "./delete_task_dialog.svelte";
  import { due_label, due_label_class } from "./due-label";

  interface CardItemProps {
    task: Task;
    index: number;
    group?: string | number;
    data?: { group: string | number };
    expand_content: boolean;
    content_open?: boolean;
    onToggleContent?: () => void;
    read_only?: boolean;
    onViewTask: () => void;
    onEditTask: () => void;
    onDuplicateTask: () => void;
    onSaveAsTemplate?: () => void;
    onExportTask?: () => void;
    onDeleteTask: () => void;
    onArchiveTask: () => void;
    onTogglePin?: () => void;
    onFloatTask?: () => void;
  }

  let {
    task,
    index,
    group,
    data,
    expand_content,
    content_open = false,
    onToggleContent,
    read_only = false,
    onViewTask = () => {},
    onEditTask = () => {},
    onDuplicateTask = () => {},
    onSaveAsTemplate,
    onExportTask = () => {},
    onDeleteTask = () => {},
    onArchiveTask = () => {},
    onTogglePin,
    onFloatTask,
  }: CardItemProps = $props();

  const { ref, isDragging, isDropTarget } = useSortable({
    id: () => task.id,
    index: () => index,
    type: "item",
    accept: "item",
    group: () => group,
    data: () => data,
    disabled: () => read_only,
  });

  let prevent_menu_focus_restore = false;

  function restore_menu_focus(event: Event) {
    if (prevent_menu_focus_restore) {
      event.preventDefault();
    }
    prevent_menu_focus_restore = false;
  }
  let has_expandable_content = $derived(
    task.items.length > 0 || task.description.trim().length > 0,
  );
  let delete_confirm_open = $state(false);
  let move_dialog_open = $state(false);
  let due_metadata = $derived(
    task.due_time
      ? due_label(task.due_time, new Date(), task.due_at_midnight)
      : undefined,
  );
</script>

<ContextMenu.Root>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <ContextMenu.Trigger
    data-task-card
    class="group relative max-w-full select-none overflow-hidden"
    {@attach ref}
    ondblclick={read_only ? undefined : onViewTask}
  >
    <Card.Root
      data-task-card-surface
      class="relative my-1 w-full gap-2 border-2 py-3 transition-[border-color,box-shadow] duration-150 hover:border-muted-foreground/40 hover:shadow-sm {read_only
        ? 'cursor-pointer'
        : ''}"
      role={read_only ? "button" : undefined}
      tabindex={read_only ? 0 : undefined}
      onclick={read_only ? onViewTask : undefined}
      onkeydown={read_only
        ? (event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onViewTask();
            }
          }
        : undefined}
    >
      <Card.Header class="w-full px-4">
        <Card.Title
          class="break-normal [overflow-wrap:anywhere] {onTogglePin &&
          !read_only
            ? 'pr-24'
            : 'pr-16'}"
          style="color: {display_task_color(task.color)};"
        >
          {task.title}
        </Card.Title>
        <Card.Description>
          {#if read_only && task.pinned}
            <span
              class="mr-3 inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            >
              <PinIcon class="size-3.5 fill-current" />
            </span>
          {/if}
          {#if due_metadata != undefined}
            <span
              class={`inline-flex items-center gap-1.5 text-xs ${due_label_class(
                due_metadata.status,
              )}`}
            >
              {#if due_metadata.status === "overdue"}
                <CircleAlertIcon class="size-3.5 shrink-0" />
              {:else}
                <ClockIcon class="size-3.5 shrink-0" />
              {/if}
              <span>{due_metadata.text}</span>
            </span>
          {/if}
          {#if task.recurrence}
            <span
              class="ml-3 inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            >
              <Repeat2Icon class="size-3.5 shrink-0" />
              <span>{recurrence_label(task.recurrence)}</span>
            </span>
          {/if}
        </Card.Description>
      </Card.Header>
      {#if !read_only}
        <div
          class="absolute top-3 right-3 z-20 flex items-start gap-1"
          onpointerdown={(event) => event.stopPropagation()}
          ondblclick={(event) => event.stopPropagation()}
        >
          {#if onToggleContent && has_expandable_content && !expand_content}
            <Button
              variant="ghost"
              size="icon-sm"
              class="size-6 text-muted-foreground hover:bg-muted/50 hover:text-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
              aria-label={content_open ? m.task_collapse() : m.task_expand()}
              title={content_open ? m.task_collapse() : m.task_expand()}
              onpointerup={(event) => event.currentTarget.blur()}
              onclick={() => {
                onToggleContent?.();
              }}
            >
              {#if content_open}
                <ChevronUpIcon class="size-4" />
              {:else}
                <ChevronDownIcon class="size-4" />
              {/if}
            </Button>
          {/if}
          <div class="flex flex-row-reverse items-center gap-1">
            <DropdownMenu.Root>
              <DropdownMenu.Trigger>
                {#snippet child({ props })}
                  <Button
                    {...props}
                    variant="ghost"
                    size="icon-sm"
                    class="size-6 text-muted-foreground hover:bg-muted/50 hover:text-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 aria-expanded:opacity-100"
                    aria-label={m.task_menu()}
                    title={m.task_menu()}
                  >
                    <EllipsisIcon class="size-4" />
                  </Button>
                {/snippet}
              </DropdownMenu.Trigger>
              <DropdownMenu.Content
                align="end"
                class="min-w-36"
                onCloseAutoFocus={restore_menu_focus}
              >
                {@render menu_items(false)}
              </DropdownMenu.Content>
            </DropdownMenu.Root>
            {#if onTogglePin}
              <Button
                variant="ghost"
                size="icon-sm"
                class="size-6 text-muted-foreground hover:bg-muted/50 hover:text-foreground opacity-0 transition-opacity aria-pressed:opacity-100 group-hover:opacity-100 focus-visible:opacity-100"
                aria-label={task.pinned ? m.task_unpin() : m.task_pin()}
                aria-pressed={task.pinned ?? false}
                title={task.pinned ? m.task_unpin() : m.task_pin()}
                onclick={(event) => {
                  if (event.detail > 0) {
                    event.currentTarget.blur();
                  }
                  onTogglePin();
                }}
              >
                <PinIcon class="size-3.5 {task.pinned ? '' : 'rotate-45'}" />
              </Button>
            {/if}
          </div>
        </div>
      {/if}
      {#if task.items.length > 0 || (task.description.trim().length > 0 && (expand_content || content_open))}
        <Card.Content
          class="w-full break-normal [overflow-wrap:anywhere] px-4 text-zinc-600 dark:text-zinc-400"
        >
          {#if task.items.length > 0}
            <ChecklistDisplay
              items={task.items}
              show_items={expand_content || content_open}
            />
          {/if}
          {#if task.description.trim().length > 0}
            <Collapsible.Root open={expand_content || content_open}>
              <Collapsible.Content>
                <div
                  class={task.items.length > 0
                    ? "mt-3 rounded-md bg-muted/40 px-3 py-2"
                    : "rounded-md bg-muted/40 px-3 py-2"}
                >
                  <Markdown md={task.description} compact />
                </div>
              </Collapsible.Content>
            </Collapsible.Root>
          {/if}
        </Card.Content>
      {/if}
      {#if task.labels.length > 0}
        <Card.Footer class="px-4">
          <div class="flex flex-wrap gap-1.5 w-full">
            {#each task.labels as lbl (lbl)}
              <LabelBadge label={lbl} />
            {/each}
          </div>
        </Card.Footer>
      {/if}
    </Card.Root>

    {#if !read_only}
      <DeleteTaskDialog
        bind:open={delete_confirm_open}
        task_title={task.title}
        onConfirm={onDeleteTask}
      />
    {/if}
  </ContextMenu.Trigger>
  <ContextMenu.Content class="min-w-36" onCloseAutoFocus={restore_menu_focus}>
    {@render menu_items(true)}
  </ContextMenu.Content>
</ContextMenu.Root>

{#snippet menu_items(context: boolean)}
  {@const Item = context ? ContextMenu.Item : DropdownMenu.Item}
  <TaskActionItems
    {context}
    onView={onViewTask}
    onEdit={read_only ? undefined : onEditTask}
    onArchive={read_only
      ? undefined
      : () => {
          // The archived trigger disappears; avoid restoring focus to it.
          prevent_menu_focus_restore = true;
          onArchiveTask();
        }}
    onDelete={read_only ? undefined : () => (delete_confirm_open = true)}
  >
    {#snippet extra()}
      {#if !read_only}
        {#if onFloatTask}
          <Item onclick={onFloatTask}>
            <PictureInPicture2Icon />
            {m.task_float_on_desktop()}
          </Item>
        {/if}
        <Item onclick={onDuplicateTask}>
          <CopyIcon />
          {m.task_duplicate_action()}
        </Item>
        <Item
          onclick={() => {
            move_dialog_open = true;
          }}
        >
          <MoveRightIcon />
          {m.task_move_board()}
        </Item>
        {#if onSaveAsTemplate}
          <Item onclick={onSaveAsTemplate}>
            <LayoutTemplateIcon />
            {m.task_template_save()}
          </Item>
        {/if}
        <Item onclick={onExportTask}>
          <Share2Icon />
          {m.task_share()}
        </Item>
      {/if}
    {/snippet}
  </TaskActionItems>
{/snippet}

{#if move_dialog_open}
  <MoveTaskDialog bind:open={move_dialog_open} {task} />
{/if}
