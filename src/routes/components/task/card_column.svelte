<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatNumber } from "$lib/i18n";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { useDroppable } from "@dnd-kit-svelte/svelte";
  import { useSortable } from "@dnd-kit-svelte/svelte/sortable";
  import { CollisionPriority } from "@dnd-kit/abstract";

  import CirclePlus from "@lucide/svelte/icons/circle-plus";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import ArrowUpDownIcon from "@lucide/svelte/icons/arrow-up-down";
  import ClipboardPasteIcon from "@lucide/svelte/icons/clipboard-paste";

  import EditableLabel from "../editable_label.svelte";

  import type { Column, ColumnSort } from "../../type/column.svelte";
  import type { Snippet } from "svelte";

  import { cn } from "$lib/utils";

  interface CardColumnProps {
    children: Snippet<[]>;
    column: Column;
    index: number;
    numberTasks: number;
    onDeleteColumn: () => void;
    onUpdateColumnName: (name: string) => void;
    onAddTask: () => void;
    onImportTask: () => void;
    onArchiveAllTasks: () => void;
    onUpdateSort: (sort: ColumnSort) => void;
    read_only?: boolean;
  }

  let {
    children,
    column,
    index,
    numberTasks,
    onDeleteColumn,
    onUpdateColumnName = (_) => {},
    onAddTask = () => {},
    onImportTask = () => {},
    onArchiveAllTasks = () => {},
    onUpdateSort = () => {},
    read_only = false,
  }: CardColumnProps = $props();

  const { ref, isDragging } = useSortable({
    id: () => column.id,
    index: () => index,
    type: "column",
    accept: ["column"],
    collisionPriority: CollisionPriority.Low,
    disabled: () => read_only,
  });

  const droppable = useDroppable({
    id: () => `empty_${column.id}`,
    type: "empty_region",
    accept: ["item"],
  });

  let delete_confirm_open = $state(false);

  function handle_column_double_click(event: MouseEvent) {
    if (read_only) {
      return;
    }
    if (event.button !== 0) {
      return;
    }

    const target = event.target;
    if (
      !(target instanceof Element) ||
      target.closest("[data-task-card-surface]")
    ) {
      return;
    }

    const selection = window.getSelection();
    if (selection && !selection.isCollapsed) {
      return;
    }

    event.preventDefault();
    onAddTask();
  }
</script>

<div class="relative select-none" {@attach ref}>
  <ContextMenu.Root>
    <ContextMenu.Trigger class="h-full">
      <Card.Root
        class={cn(
          "column-card h-full w-xs gap-3 border-2 py-3 transition-[border-color,box-shadow] duration-150",
          column.color,
        )}
      >
        <Card.Header class="px-4">
          <Card.Title class="flex min-h-8 w-full min-w-0 flex-row items-center">
            <div class="flex-grow min-w-0 h-full">
              <EditableLabel
                value={column.name}
                placeholder={m.common_name()}
                disabled={read_only}
                onupdate={(new_value: string) => {
                  onUpdateColumnName?.(new_value);
                }}
              ></EditableLabel>
            </div>
            <Badge
              variant="secondary"
              class="mx-2 shrink-0 font-normal tabular-nums"
              aria-label={m.task_count({ count: numberTasks })}
            >
              {formatNumber(numberTasks)}
            </Badge>
            {#if column.sort_order !== "custom"}
              <button
                type="button"
                class="mr-1 inline-flex shrink-0 items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs font-normal text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground"
                title={column.sort_order === "due_date_asc"
                  ? m.column_due_asc_hint()
                  : m.column_due_desc_hint()}
                aria-label={column.sort_order === "due_date_asc"
                  ? m.column_due_asc_accessible()
                  : m.column_due_desc_accessible()}
                onpointerdown={(event) => event.stopPropagation()}
                onclick={() => onUpdateSort("custom")}
              >
                <ArrowUpDownIcon class="size-3" />
                {m.task_due_date()}
                {column.sort_order === "due_date_asc" ? "↑" : "↓"}
              </button>
            {/if}
            {#if !read_only}
              <Button
                variant="ghost"
                size="icon-sm"
                class="mx-0 size-8"
                onclick={onAddTask}
                aria-label={m.column_add_named_task({ name: column.name })}
                title={m.column_add_named_task({ name: column.name })}
              >
                <CirclePlus />
              </Button>
            {/if}
          </Card.Title>
        </Card.Header>
        <Card.Content
          class="px-3 h-full w-full"
          ondblclick={handle_column_double_click}
        >
          <div class="flex flex-col w-full h-full">
            {@render children()}
            <div class="flex-grow" {@attach droppable.ref}></div>
          </div>
        </Card.Content>
      </Card.Root>
    </ContextMenu.Trigger>
    {#if !read_only}
      <ContextMenu.Content>
        <ContextMenu.Item onclick={onAddTask}>
          <CirclePlus />
          {m.task_create()}
        </ContextMenu.Item>
        <ContextMenu.Item onclick={onImportTask}>
          <ClipboardPasteIcon />
          {m.task_import()}
        </ContextMenu.Item>
        {#if numberTasks > 0}
          <ContextMenu.Item onclick={onArchiveAllTasks}>
            <ArchiveIcon />
            {m.column_archive_tasks()}
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Sub>
          <ContextMenu.SubTrigger>
            <ArrowUpDownIcon />
            {m.common_sort()}
          </ContextMenu.SubTrigger>
          <ContextMenu.SubContent>
            <ContextMenu.RadioGroup
              value={column.sort_order}
              onValueChange={(value) => onUpdateSort(value as ColumnSort)}
            >
              <ContextMenu.RadioItem value="custom"
                >{m.common_custom()}</ContextMenu.RadioItem
              >
              <ContextMenu.RadioItem value="due_date_asc"
                >{m.column_due_asc()}</ContextMenu.RadioItem
              >
              <ContextMenu.RadioItem value="due_date_desc"
                >{m.column_due_desc()}</ContextMenu.RadioItem
              >
            </ContextMenu.RadioGroup>
          </ContextMenu.SubContent>
        </ContextMenu.Sub>
        <ContextMenu.Separator />
        <ContextMenu.Item
          variant="destructive"
          onclick={() => {
            if (numberTasks > 0) {
              delete_confirm_open = true;
            } else {
              onDeleteColumn();
            }
          }}
        >
          <TrashIcon />
          {m.column_delete()}</ContextMenu.Item
        >
      </ContextMenu.Content>
    {/if}
  </ContextMenu.Root>

  <AlertDialog.Root bind:open={delete_confirm_open}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>{m.column_delete()}</AlertDialog.Title>
        <AlertDialog.Description>
          {m.column_delete_confirm()}
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>{m.common_cancel()}</AlertDialog.Cancel>
        <AlertDialog.Action
          variant="destructive"
          onclick={() => {
            delete_confirm_open = false;
            onDeleteColumn();
          }}>{m.common_delete()}</AlertDialog.Action
        >
      </AlertDialog.Footer>
    </AlertDialog.Content>
  </AlertDialog.Root>
</div>

<style>
  :global(.column-card:hover:not(:has([data-task-card]:hover))) {
    border-color: color-mix(in oklab, var(--muted-foreground) 40%, transparent);
    box-shadow: var(--shadow-sm);
  }
</style>
