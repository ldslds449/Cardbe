<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatDate, formatDateTime } from "$lib/i18n";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Collapsible from "$lib/components/ui/collapsible/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { useSortable } from "@dnd-kit-svelte/svelte/sortable";

  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import Share2Icon from "@lucide/svelte/icons/share-2";
  import PinIcon from "@lucide/svelte/icons/pin";
  import MoveRightIcon from "@lucide/svelte/icons/move-right";
  import MoveTaskDialog from "../dialog/move_task_dialog.svelte";

  import { recurrence_label, type Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";
  import Markdown from "../markdown.svelte";
  import LabelBadge from "../label_badge.svelte";
  import ChecklistDisplay from "./checklist_display.svelte";
  import DeleteTaskDialog from "./delete_task_dialog.svelte";
  import { due_status } from "./due-status";

  interface CardItemProps {
    task: Task;
    index: number;
    group?: string | number;
    data?: { group: string | number };
    expand_content: boolean;
    read_only?: boolean;
    onViewTask: () => void;
    onEditTask: () => void;
    onDuplicateTask: () => void;
    onSaveAsTemplate?: () => void;
    onExportTask?: () => void;
    onDeleteTask: () => void;
    onArchiveTask: () => void;
    onTogglePin?: () => void;
  }

  let {
    task,
    index,
    group,
    data,
    expand_content,
    read_only = false,
    onViewTask = () => {},
    onEditTask = () => {},
    onDuplicateTask = () => {},
    onSaveAsTemplate,
    onExportTask = () => {},
    onDeleteTask = () => {},
    onArchiveTask = () => {},
    onTogglePin,
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

  let show_content = $state(false);
  let prevent_menu_focus_restore = false;
  let has_expandable_content = $derived(
    task.items.length > 0 || task.description.trim().length > 0,
  );
  let delete_confirm_open = $state(false);
  let move_dialog_open = $state(false);
  let due_metadata = $derived(
    (() => {
      if (!task.due_time) {
        return undefined;
      }

      const now = new Date();
      const include_year = task.due_time.getFullYear() !== now.getFullYear();
      const options: Intl.DateTimeFormatOptions = {
        month: "short",
        day: "numeric",
        year: include_year ? "numeric" : undefined,
      };
      const has_time =
        task.due_time.getHours() !== 0 ||
        task.due_time.getMinutes() !== 0 ||
        task.due_time.getSeconds() !== 0;
      const date = has_time
        ? formatDateTime(task.due_time, {
            ...options,
            hour: "2-digit",
            minute: "2-digit",
          })
        : formatDate(task.due_time, options);
      const status = due_status(task.due_time, now);
      return {
        text:
          status === "today"
            ? m.task_due_today({ date })
            : status === "soon"
              ? m.task_due_soon({ date })
              : date,
        status,
      };
    })(),
  );

  let previous_expand_content = $state<boolean | undefined>(undefined);
  $effect(() => {
    if (
      previous_expand_content !== undefined &&
      expand_content !== previous_expand_content
    ) {
      show_content = false;
    }
    previous_expand_content = expand_content;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  data-task-card
  class="group relative max-w-full select-none overflow-hidden"
  {@attach ref}
  ondblclick={read_only ? undefined : onViewTask}
  oncontextmenu={(event) => {
    if (read_only) {
      return;
    }
    event.preventDefault();
    onEditTask();
  }}
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
        class="break-all {onTogglePin && !read_only ? 'pr-24' : 'pr-16'}"
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
            class={`inline-flex items-center gap-1.5 text-xs ${
              due_metadata.status === "overdue"
                ? "text-destructive"
                : due_metadata.status === "today"
                  ? "font-medium text-warning"
                  : due_metadata.status === "soon"
                    ? "font-medium text-warning"
                    : "text-muted-foreground"
            }`}
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
        {#if has_expandable_content && !expand_content}
          <Button
            variant="ghost"
            size="icon-sm"
            class="size-6 text-muted-foreground hover:bg-muted/50 hover:text-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
            aria-label={show_content ? m.task_collapse() : m.task_expand()}
            title={show_content ? m.task_collapse() : m.task_expand()}
            onpointerup={(event) => event.currentTarget.blur()}
            onclick={() => {
              show_content = !show_content;
            }}
          >
            {#if show_content}
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
              onCloseAutoFocus={(event) => {
                if (prevent_menu_focus_restore) {
                  event.preventDefault();
                }
                prevent_menu_focus_restore = false;
              }}
            >
              <DropdownMenu.Item onclick={onViewTask}>
                <FullscreenIcon />
                {m.task_view_details()}
              </DropdownMenu.Item>
              <DropdownMenu.Item onclick={onEditTask}>
                <PencilIcon />
                {m.task_edit_action()}
              </DropdownMenu.Item>
              <DropdownMenu.Item onclick={onDuplicateTask}>
                <CopyIcon />
                {m.task_duplicate_action()}
              </DropdownMenu.Item>
              <DropdownMenu.Item
                onclick={() => {
                  move_dialog_open = true;
                }}
              >
                <MoveRightIcon />
                {m.task_move_board()}
              </DropdownMenu.Item>
              {#if onSaveAsTemplate}
                <DropdownMenu.Item onclick={onSaveAsTemplate}>
                  <LayoutTemplateIcon />
                  {m.task_template_save()}
                </DropdownMenu.Item>
              {/if}
              <DropdownMenu.Item onclick={onExportTask}>
                <Share2Icon />
                {m.task_share()}
              </DropdownMenu.Item>
              <DropdownMenu.Item
                onSelect={() => {
                  // This trigger disappears with the archived card. Restoring
                  // focus to it can make the scroll viewport jump to the top.
                  prevent_menu_focus_restore = true;
                  onArchiveTask();
                }}
              >
                <ArchiveIcon />
                {m.task_archive_action()}
              </DropdownMenu.Item>
              <DropdownMenu.Separator />
              <DropdownMenu.Item
                variant="destructive"
                onclick={() => {
                  delete_confirm_open = true;
                }}
              >
                <TrashIcon />
                {m.task_delete()}
              </DropdownMenu.Item>
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
    {#if task.items.length > 0 || (task.description.trim().length > 0 && (expand_content || show_content))}
      <Card.Content
        class="w-full break-all px-4 text-zinc-600 dark:text-zinc-400"
      >
        {#if task.items.length > 0}
          <ChecklistDisplay
            items={task.items}
            show_items={expand_content || show_content}
          />
        {/if}
        {#if task.description.trim().length > 0}
          <Collapsible.Root open={expand_content || show_content}>
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
</div>
{#if move_dialog_open}
  <MoveTaskDialog bind:open={move_dialog_open} {task} />
{/if}
