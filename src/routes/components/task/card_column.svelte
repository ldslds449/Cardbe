<script lang="ts">
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
  if (read_only) return;
  if (event.button !== 0) return;

  const target = event.target;
  if (
    !(target instanceof Element) ||
    target.closest("[data-task-card-surface]")
  ) {
    return;
  }

  const selection = window.getSelection();
  if (selection && !selection.isCollapsed) return;

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
                placeholder="Name"
                disabled={read_only}
                onupdate={(new_value: string) => {
                                    onUpdateColumnName?.(new_value);
                                }}
              ></EditableLabel>
            </div>
            {#if column.sort_order !== "custom"}
              <button
                type="button"
                class="mr-1 inline-flex shrink-0 items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs font-normal text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground"
                title={column.sort_order === "due_date_asc"
                                    ? "Due date: earliest first. Click to switch to Custom."
                                    : "Due date: latest first. Click to switch to Custom."}
                aria-label={column.sort_order === "due_date_asc"
                                    ? "Due date: earliest first. Switch to Custom sorting."
                                    : "Due date: latest first. Switch to Custom sorting."}
                onpointerdown={(event) => event.stopPropagation()}
                onclick={() => onUpdateSort("custom")}
              >
                <ArrowUpDownIcon class="size-3" />
                Due {column.sort_order === "due_date_asc" ? "↑" : "↓"}
              </button>
            {/if}
            {#if !read_only}
              <Button
                variant="ghost"
                size="icon-sm"
                class="mx-0 size-8"
                onclick={onAddTask}
                aria-label={`Add task to ${column.name}`}
                title={`Add task to ${column.name}`}
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
          Add Task
        </ContextMenu.Item>
        <ContextMenu.Item onclick={onImportTask}>
          <ClipboardPasteIcon />
          Import Task
        </ContextMenu.Item>
        {#if numberTasks > 0}
          <ContextMenu.Item onclick={onArchiveAllTasks}>
            <ArchiveIcon />
            Archive All Tasks
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Sub>
          <ContextMenu.SubTrigger>
            <ArrowUpDownIcon />
            Sort
          </ContextMenu.SubTrigger>
          <ContextMenu.SubContent>
            <ContextMenu.RadioGroup
              value={column.sort_order}
              onValueChange={(value) => onUpdateSort(value as ColumnSort)}
            >
              <ContextMenu.RadioItem value="custom"
                >Custom</ContextMenu.RadioItem
              >
              <ContextMenu.RadioItem value="due_date_asc"
                >Due date: earliest first</ContextMenu.RadioItem
              >
              <ContextMenu.RadioItem value="due_date_desc"
                >Due date: latest first</ContextMenu.RadioItem
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
          Delete Column</ContextMenu.Item
        >
      </ContextMenu.Content>
    {/if}
  </ContextMenu.Root>

  <AlertDialog.Root bind:open={delete_confirm_open}>
    <AlertDialog.Content>
      <AlertDialog.Header>
        <AlertDialog.Title>Delete Column</AlertDialog.Title>
        <AlertDialog.Description>
          Do you want to delete the column and all contained tasks?
        </AlertDialog.Description>
      </AlertDialog.Header>
      <AlertDialog.Footer>
        <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
        <AlertDialog.Action
          onclick={() => {
                        delete_confirm_open = false;
                        onDeleteColumn();
                    }}
          >Confirm</AlertDialog.Action
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
