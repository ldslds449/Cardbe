<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import * as AlertDialog from "#lib/components/ui/alert-dialog/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { useDroppable } from "@dnd-kit-svelte/svelte";
  import { useSortable } from "@dnd-kit-svelte/svelte/sortable";
  import { CollisionPriority } from "@dnd-kit/abstract";

  import CirclePlus from "@lucide/svelte/icons/circle-plus";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import ArrowUpDownIcon from "@lucide/svelte/icons/arrow-up-down";
  import ClipboardPasteIcon from "@lucide/svelte/icons/clipboard-paste";
  import SquareCheckIcon from "@lucide/svelte/icons/square-check";
  import BellOffIcon from "@lucide/svelte/icons/bell-off";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";

  import EditableLabel from "../editable_label.svelte";
  import ColumnSortDialog from "../dialog/column_sort_dialog.svelte";
  import ResizeHandle from "../resize_handle.svelte";
  import { column_width_bounds } from "../../layout-widths.svelte";

  import { type Column, type SortRule } from "../../type/column.svelte";
  import {
    active_sort_rules,
    sort_field_label,
    sort_rules_hint,
  } from "../../utils/column-sort";
  import type { Snippet } from "svelte";

  import { cn } from "#lib/utils.js";

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
    onSelectAllTasks?: () => void;
    onUpdateSort: (sort_rules: SortRule[]) => void;
    width?: number;
    onResize?: (width: number, commit: boolean) => void;
    read_only?: boolean;
    notify_muted?: boolean;
    onToggleNotifyMuted?: (muted: boolean) => void;
    /** Whether this is the board's done column. */
    done?: boolean;
    onToggleDone?: (done: boolean) => void;
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
    onSelectAllTasks,
    onUpdateSort = () => {},
    width = column_width_bounds.default,
    onResize,
    read_only = false,
    notify_muted = false,
    onToggleNotifyMuted,
    done = false,
    onToggleDone,
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
  let sort_dialog_open = $state(false);

  const sort_rules = $derived(active_sort_rules(column.sort_rules));
  const primary_sort = $derived(sort_rules[0]);
  const sort_hint = $derived(sort_rules_hint(column.sort_rules));

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
          "column-card h-full gap-3 border-2 py-3 transition-[border-color,box-shadow] duration-150",
          column.color,
        )}
        style="width: {width}px"
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
            {#if done}
              <Badge
                variant="outline"
                class="mr-1 shrink-0 gap-1 font-normal text-muted-foreground"
                title={m.column_done_hint()}
              >
                <CircleCheckIcon class="size-3" aria-hidden="true" />
                {m.column_done_badge()}
              </Badge>
            {/if}
            {#if notify_muted}
              <span
                class="mr-1 inline-flex shrink-0 text-muted-foreground"
                role="img"
                title={m.column_notify_muted_hint()}
                aria-label={m.column_notify_muted_hint()}
              >
                <BellOffIcon class="size-3.5" />
              </span>
            {/if}
            {#if primary_sort}
              <button
                type="button"
                class="mr-1 inline-flex min-w-0 shrink items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs font-normal text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground disabled:pointer-events-none"
                title={sort_hint}
                aria-label={sort_hint}
                disabled={read_only}
                onpointerdown={(event) => event.stopPropagation()}
                onclick={() => (sort_dialog_open = true)}
              >
                <ArrowUpDownIcon class="size-3 shrink-0" />
                <span class="truncate"
                  >{sort_field_label(primary_sort.field)}</span
                >
                <span aria-hidden="true"
                  >{primary_sort.direction === "asc" ? "↑" : "↓"}</span
                >
                {#if sort_rules.length > 1}
                  <span aria-hidden="true" class="tabular-nums"
                    >{m.column_sort_more({
                      count: formatNumber(sort_rules.length - 1),
                    })}</span
                  >
                {/if}
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
    {#if onResize}
      <ResizeHandle
        {width}
        bounds={column_width_bounds}
        label={m.layout_resize_column()}
        class="-end-3.5"
        onresize={onResize}
      />
    {/if}
    {#if !read_only || onToggleNotifyMuted}
      <ContextMenu.Content>
        {#if !read_only}
          <ContextMenu.Item onclick={onAddTask}>
            <CirclePlus />
            {m.task_create()}
          </ContextMenu.Item>
          <ContextMenu.Item onclick={onImportTask}>
            <ClipboardPasteIcon />
            {m.task_import()}
          </ContextMenu.Item>
          {#if numberTasks > 0}
            {#if onSelectAllTasks}
              <ContextMenu.Item onclick={onSelectAllTasks}>
                <SquareCheckIcon />
                {m.column_select_tasks()}
              </ContextMenu.Item>
            {/if}
            <ContextMenu.Item onclick={onArchiveAllTasks}>
              <ArchiveIcon />
              {m.column_archive_tasks()}
            </ContextMenu.Item>
          {/if}
          <ContextMenu.Item onclick={() => (sort_dialog_open = true)}>
            <ArrowUpDownIcon />
            {m.column_sort_open()}
          </ContextMenu.Item>
          {#if onToggleDone}
            <ContextMenu.Item onclick={() => onToggleDone(!done)}>
              <CircleCheckIcon />
              <div class="flex flex-col">
                <span
                  >{done
                    ? m.column_done_unset()
                    : m.column_done_set_action()}</span
                >
                <span class="text-xs text-muted-foreground"
                  >{m.column_done_hint()}</span
                >
              </div>
            </ContextMenu.Item>
          {/if}
        {/if}
        {#if onToggleNotifyMuted}
          <ContextMenu.CheckboxItem
            checked={notify_muted}
            onCheckedChange={(checked) => onToggleNotifyMuted(checked)}
          >
            {m.column_notify_mute()}
          </ContextMenu.CheckboxItem>
        {/if}
        {#if !read_only}
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
        {/if}
      </ContextMenu.Content>
    {/if}
  </ContextMenu.Root>

  <ColumnSortDialog
    bind:open={sort_dialog_open}
    column_name={column.name}
    rules={column.sort_rules}
    onApply={onUpdateSort}
  />

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
