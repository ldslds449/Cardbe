<script lang="ts">
  import TaskActionItems from "../task/task_action_items.svelte";
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatNumber } from "$lib/i18n";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { useSortable } from "@dnd-kit-svelte/svelte/sortable";
  import type { Attachment } from "svelte/attachments";

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
  import WorkflowIcon from "@lucide/svelte/icons/workflow";
  import SquareCheckIcon from "@lucide/svelte/icons/square-check";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import MoveTaskDialog from "../dialog/move_task_dialog.svelte";
  import { cn } from "$lib/utils";

  import { recurrence_label, type Task } from "../../type/task.svelte";
  import { tab_label, type FlowTab } from "../flow/flow";
  import { display_task_color } from "../../utils/task-color";
  import Markdown from "../markdown.svelte";
  import CollapsibleBlock from "../collapsible_block.svelte";
  import { markdown_stats } from "../../utils/markdown-outline";
  import LabelBadge from "../label_badge.svelte";
  import PriorityIndicator from "./priority_indicator.svelte";
  import ChecklistDisplay from "./checklist_display.svelte";
  import DeleteTaskDialog from "./delete_task_dialog.svelte";
  import { due_label, due_label_class } from "./due-label";
  import {
    claim_context_menu,
    release_context_menu,
  } from "./exclusive-context-menu";

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
    onLoadFlowTabs?: () => Promise<FlowTab[] | null>;
    onAddToFlow?: (tab: FlowTab) => void;
    selected?: boolean;
    selecting?: boolean;
    onSelect?: (event: MouseEvent | null) => void;
    /** Attached to the card element, e.g. to window a long column. */
    attach?: Attachment<HTMLElement>;
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
    onLoadFlowTabs,
    onAddToFlow,
    selected = false,
    selecting = false,
    onSelect,
    attach,
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
  let context_menu_open = $state(false);
  const close_context_menu = () => (context_menu_open = false);

  function handle_context_menu_open(open: boolean) {
    if (open) {
      claim_context_menu(close_context_menu);
    } else {
      release_context_menu(close_context_menu);
    }
    void load_flow_tabs(open);
  }

  $effect(() => () => release_context_menu(close_context_menu));

  function restore_menu_focus(event: Event) {
    if (prevent_menu_focus_restore) {
      event.preventDefault();
    }
    prevent_menu_focus_restore = false;
  }
  let has_expandable_content = $derived(
    task.items.length > 0 || task.description.trim().length > 0,
  );
  // Buttons in the top-right corner: content toggle, menu, and pin. Each is
  // 1.5rem wide with 0.25rem gaps; the group sits 0.25rem past the title.
  let header_button_count = $derived(
    read_only
      ? 0
      : 1 +
          (onTogglePin ? 1 : 0) +
          (onToggleContent && has_expandable_content && !expand_content
            ? 1
            : 0),
  );
  let description_length = $derived(
    markdown_stats(task.description.trim()).characters,
  );
  let delete_confirm_open = $state(false);
  let move_dialog_open = $state(false);
  // Loaded when the menu opens; a single tab is added without a submenu.
  let flow_tabs = $state.raw<FlowTab[] | null>(null);
  async function load_flow_tabs(open: boolean) {
    if (open && !read_only && onLoadFlowTabs) {
      flow_tabs = null;
      flow_tabs = await onLoadFlowTabs();
    }
  }
  let due_metadata = $derived(
    task.due_time
      ? due_label(task.due_time, new Date(), task.due_at_midnight)
      : undefined,
  );

  // Ctrl/Cmd or Shift starts a selection; once one exists, plain clicks extend it.
  function is_select_click(event: MouseEvent) {
    return (
      onSelect !== undefined &&
      (event.ctrlKey || event.metaKey || event.shiftKey || selecting) &&
      !(
        event.target instanceof Element &&
        event.target.closest("button, a, input, label")
      )
    );
  }

  function handle_select_click(event: MouseEvent) {
    if (is_select_click(event)) {
      event.preventDefault();
      onSelect?.(event);
    }
  }

  // dnd-kit makes the card focusable, and Chromium treats a held Ctrl/Shift as
  // keyboard input, so focusing on mousedown would flash a focus ring.
  function handle_select_mousedown(event: MouseEvent) {
    if (event.button === 0 && is_select_click(event)) {
      event.preventDefault();
    }
  }
</script>

<ContextMenu.Root
  bind:open={context_menu_open}
  onOpenChange={handle_context_menu_open}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <ContextMenu.Trigger
    data-task-card
    class="group relative max-w-full select-none overflow-hidden outline-none"
    {@attach ref}
    {@attach attach}
    ondblclick={read_only || selecting ? undefined : onViewTask}
  >
    <Card.Root
      data-task-card-surface
      data-selected={selected || undefined}
      class={cn(
        // Dragging makes the wrapper focusable; its square outline is
        // replaced by the rounded card's border.
        "relative my-1 w-full gap-2 border-2 py-3 transition-[border-color,box-shadow] duration-150 hover:border-muted-foreground/40 hover:shadow-sm group-focus-visible:border-ring",
        read_only && "cursor-pointer",
        selected && "border-primary bg-primary/5 hover:border-primary",
      )}
      role={read_only ? "button" : undefined}
      tabindex={read_only ? 0 : undefined}
      onclick={read_only ? onViewTask : handle_select_click}
      onmousedown={read_only ? undefined : handle_select_mousedown}
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
          class="break-normal [overflow-wrap:anywhere]"
          style="color: {display_task_color(task.color)};"
        >
          {#if header_button_count > 0}
            <!-- Only the first line, beside the hover buttons, leaves room for them. -->
            <span
              aria-hidden="true"
              class="float-right h-[1em]"
              style:width="{header_button_count * 1.75 - 0.25}rem"
            ></span>
          {/if}
          {task.title}
        </Card.Title>
        <!-- Priority leads, the due date (with its repeat) follows, and the
             description size comes last as secondary information. -->
        <Card.Description class="flex flex-wrap items-center gap-x-2 gap-y-1">
          {#if read_only && task.pinned}
            <span
              class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            >
              <PinIcon class="size-3.5 fill-current" />
            </span>
          {/if}
          {#if task.priority}
            <PriorityIndicator priority={task.priority} />
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
              {#if task.recurrence}
                <span
                  class="inline-flex text-muted-foreground"
                  title={recurrence_label(task.recurrence)}
                >
                  <Repeat2Icon class="size-3.5 shrink-0" />
                  <span class="sr-only"
                    >{recurrence_label(task.recurrence)}</span
                  >
                </span>
              {/if}
            </span>
          {:else if task.recurrence}
            <span
              class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            >
              <Repeat2Icon class="size-3.5 shrink-0" />
              <span>{recurrence_label(task.recurrence)}</span>
            </span>
          {/if}
          {#if description_length > 0 && !(expand_content || content_open)}
            <span
              class="inline-flex items-center gap-1 text-xs text-muted-foreground"
              title={m.task_description_length({ count: description_length })}
            >
              <FileTextIcon class="size-3.5 shrink-0" />
              <span class="sr-only"
                >{m.task_description_length({
                  count: description_length,
                })}</span
              >
              <span aria-hidden="true">{formatNumber(description_length)}</span>
            </span>
          {/if}
        </Card.Description>
      </Card.Header>
      {#if !read_only}
        <div
          class="absolute top-2 right-3 z-20 flex items-start gap-1"
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
            <DropdownMenu.Root onOpenChange={load_flow_tabs}>
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
              preview_limit={5}
            />
          {/if}
          <!-- Not a Collapsible: it measures each card on mount, with no animation to size. -->
          {#if task.description.trim().length > 0 && (expand_content || content_open)}
            <div
              class={task.items.length > 0
                ? "mt-3 rounded-md bg-muted/40 px-3 py-2"
                : "rounded-md bg-muted/40 px-3 py-2"}
            >
              <CollapsibleBlock
                max_height={160}
                expand_label={m.markdown_show_all()}
                collapse_label={m.markdown_show_less()}
              >
                <Markdown md={task.description} compact />
              </CollapsibleBlock>
            </div>
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
  {@const Sub = context ? ContextMenu.Sub : DropdownMenu.Sub}
  {@const SubTrigger = context
    ? ContextMenu.SubTrigger
    : DropdownMenu.SubTrigger}
  {@const SubContent = context
    ? ContextMenu.SubContent
    : DropdownMenu.SubContent}
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
        {#if onSelect}
          <Item title={m.task_selection_hint()} onclick={() => onSelect(null)}>
            <SquareCheckIcon />
            {selected ? m.task_deselect() : m.task_select()}
          </Item>
        {/if}
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
        {#if onAddToFlow && flow_tabs?.length === 1}
          {@const tab = flow_tabs[0]}
          <Item onclick={() => onAddToFlow(tab)}>
            <WorkflowIcon />
            {m.task_add_to_flow()}
          </Item>
        {:else if onAddToFlow && flow_tabs && flow_tabs.length > 1}
          <Sub>
            <SubTrigger>
              <WorkflowIcon />
              {m.task_add_to_flow()}
            </SubTrigger>
            <SubContent class="max-w-64">
              {#each flow_tabs as tab (tab.id)}
                <Item onclick={() => onAddToFlow(tab)}>
                  <span class="truncate">{tab_label(tab)}</span>
                </Item>
              {/each}
            </SubContent>
          </Sub>
        {/if}
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
