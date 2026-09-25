<script lang="ts">
  import { DragDropProvider } from "@dnd-kit-svelte/svelte";
  import SquirrelIcon from "@lucide/svelte/icons/squirrel";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";

  import { board } from "../../board.svelte";
  import type { Task } from "../../type/task.svelte";
  import CardColumn from "../task/card_column.svelte";
  import CardItem from "../task/card_item.svelte";

  type TaskPosition = { column_idx: number; task_idx: number };

  interface BoardViewProps {
    search_text?: string;
    task_expand_mode?: boolean;
    onAddColumn?: () => void;
    onAddTask?: (column_idx: number) => void;
    onImportTask?: (column_id: string) => void;
    onViewTask?: (task: Task, column_idx: number, task_idx: number) => void;
    onEditTask?: (column_idx: number, task_idx: number) => void;
    onSaveAsTemplate?: (task: Task) => void;
    onExportTask?: (task: Task) => void;
    read_only?: boolean;
  }

  let {
    search_text = "",
    task_expand_mode = true,
    onAddColumn = () => {},
    onAddTask = () => {},
    onImportTask = () => {},
    onViewTask = () => {},
    onEditTask = () => {},
    onSaveAsTemplate = () => {},
    onExportTask = () => {},
    read_only = false,
  }: BoardViewProps = $props();

  let dragged_task = $state<{ id: string; origin: TaskPosition } | null>(null);
  let dragged_column = $state<{ id: string; origin_idx: number } | null>(null);

  function find_task_position(task_id: string): TaskPosition | undefined {
    for (let column_idx = 0; column_idx < board.columns.length; column_idx++) {
      const task_idx = board.columns[column_idx].tasks.findIndex((task) => task.id === task_id);
      if (task_idx !== -1) return { column_idx, task_idx };
    }
    return undefined;
  }

  function handle_drag_start(event: any) {
    if (read_only) return;
    const source = event.operation?.source;
    const source_id = source?.id?.toString();
    if (!source_id) return;

    if (source.type === "item") {
      const origin = find_task_position(source_id);
      dragged_task = origin ? { id: source_id, origin } : null;
      dragged_column = null;
    } else if (source.type === "column") {
      const origin_idx = board.columns.findIndex((column) => column.id === source_id);
      dragged_column = origin_idx === -1 ? null : { id: source_id, origin_idx };
      dragged_task = null;
    }
  }

  function handle_drag_end(event: any) {
    if (read_only) return;
    const { source, target } = event.operation;
    const target_id = target?.id?.toString();

    if (source?.type === "item" && dragged_task) {
      if (target_id?.startsWith("empty_")) {
        const current = find_task_position(dragged_task.id);
        const destination_column_id = target_id.slice("empty_".length);
        const destination_column_idx = board.columns.findIndex(
          (column) => column.id === destination_column_id,
        );
        if (current && destination_column_idx !== -1) {
          board.move_task(
            current.column_idx,
            current.task_idx,
            destination_column_idx,
            null,
            false,
          );
        }
      }

      const final_position = find_task_position(dragged_task.id);
      if (
        final_position &&
        (final_position.column_idx !== dragged_task.origin.column_idx ||
          final_position.task_idx !== dragged_task.origin.task_idx)
      ) {
        board.persist_task_move(
          dragged_task.id,
          board.columns[final_position.column_idx].id,
          board.columns[final_position.column_idx].tasks[final_position.task_idx + 1]?.id ?? null,
        );
      }
    } else if (source?.type === "column" && dragged_column) {
      const final_column_idx = board.columns.findIndex(
        (column) => column.id === dragged_column?.id,
      );
      if (final_column_idx !== -1 && final_column_idx !== dragged_column.origin_idx) {
        board.persist_column_move(
          dragged_column.id,
          board.columns[final_column_idx + 1]?.id ?? null,
        );
      }
    }

    dragged_task = null;
    dragged_column = null;
  }

  function handle_drag_over(event: any) {
    if (read_only) return;
    const { source, target } = event.operation;
    const source_id = source?.id?.toString();
    const target_id = target?.id?.toString();
    if (!source_id || !target_id || target_id.startsWith("empty_")) return;

    if (source?.type === "column") {
      const from_col_idx = board.columns.findIndex((column) => column.id === source_id);
      const to_col_idx = board.columns.findIndex((column) => column.id === target_id);
      if (from_col_idx !== -1 && to_col_idx !== -1 && from_col_idx !== to_col_idx) {
        board.move_column(from_col_idx, to_col_idx, false);
      }
    } else if (source?.type === "item") {
      const from_position = find_task_position(source_id);
      const to_position = find_task_position(target_id);
      if (
        from_position &&
        to_position &&
        from_position.column_idx === to_position.column_idx &&
        board.columns[from_position.column_idx].sort_order !== "custom"
      ) {
        // dnd-kit optimistically reorders the DOM unless the drag-over event is
        // explicitly prevented. Returning here only skips our store update.
        event.preventDefault();
        return;
      }
      if (
        from_position &&
        to_position &&
        (from_position.column_idx !== to_position.column_idx ||
          from_position.task_idx !== to_position.task_idx)
      ) {
        board.move_task(
          from_position.column_idx,
          from_position.task_idx,
          to_position.column_idx,
          to_position.task_idx,
          false,
        );
      }
    }
  }

  function task_matches_search(task: Task): boolean {
    if (!search_text) return true;
    const search = search_text.toLowerCase();
    return (
      task.title.toLowerCase().includes(search) ||
      task.description.toLowerCase().includes(search) ||
      task.labels.some((label) => label.toLowerCase().includes(search)) ||
      task.items.some((item) => item.text.toLowerCase().includes(search))
    );
  }
</script>

{#if board.columns.length === 0}
  <Empty.Root>
    <Empty.Header>
      <Empty.Media variant="icon">
        <SquirrelIcon />
      </Empty.Media>
      <Empty.Title>No Column</Empty.Title>
      <Empty.Description>
        You haven't added any column yet. Get started by adding your first column!
      </Empty.Description>
    </Empty.Header>
    <Empty.Content>
      {#if !read_only}<Button onclick={onAddColumn}>Add Column</Button>{/if}
    </Empty.Content>
  </Empty.Root>
{:else}
  <DragDropProvider
    onDragStart={handle_drag_start}
    onDragEnd={handle_drag_end}
    onDragOver={handle_drag_over}
  >
    <div class="flex min-h-full flex-row space-x-4">
      {#each board.columns as column, column_idx (column.id)}
        <CardColumn
          index={column_idx}
          {column}
          numberTasks={column.tasks.length}
          onDeleteColumn={() => board.delete_column(column.id)}
          onUpdateColumnName={(name) => board.update_column(column.id, name)}
          onUpdateSort={(sort) => board.update_column(column.id, column.name, sort)}
          onAddTask={() => onAddTask(column_idx)}
          onImportTask={() => onImportTask(column.id)}
          onArchiveAllTasks={() => board.archive_all_tasks(column.id)}
          {read_only}
        >
          {#each column.tasks as task, task_idx (task.id)}
            {#if task_matches_search(task)}
              <CardItem
                index={task_idx}
                {task}
                group={column.id}
                data={{ group: column.id }}
                expand_content={task_expand_mode}
                onViewTask={() => onViewTask(task, column_idx, task_idx)}
                onEditTask={() => onEditTask(column_idx, task_idx)}
                onDuplicateTask={() => board.duplicate_task(task.id)}
                onSaveAsTemplate={() => onSaveAsTemplate(task)}
                onExportTask={() => onExportTask(task)}
                onDeleteTask={() => board.delete_task(task.id)}
                onArchiveTask={() => board.archive_task(task.id)}
                {read_only}
              />
            {/if}
          {/each}
        </CardColumn>
      {/each}
    </div>
  </DragDropProvider>
{/if}
