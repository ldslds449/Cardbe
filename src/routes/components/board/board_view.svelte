<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import {
    DragDropProvider,
    KeyboardSensor,
    PointerSensor,
  } from "@dnd-kit-svelte/svelte";
  import SquirrelIcon from "@lucide/svelte/icons/squirrel";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";

  import { board } from "../../board.svelte";
  import { distance_activation_constraint } from "../dnd/activation";
  import type { Task } from "../../type/task.svelte";
  import CardColumn from "../task/card_column.svelte";
  import CardItem from "../task/card_item.svelte";
  import TaskSelectionBar from "../task/task_selection_bar.svelte";
  import { task_matches_search } from "../calendar/calendar";

  type TaskPosition = { column_idx: number; task_idx: number };

  interface BoardViewProps {
    search_text?: string;
    search_task_ids?: ReadonlySet<string> | null;
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
    search_task_ids = null,
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

  let selected_ids = $state<ReadonlySet<string>>(new Set());
  let selection_anchor: string | null = null;
  // Hidden or removed tasks drop out, so actions only apply to visible cards.
  const selected_tasks = $derived(
    board.columns.flatMap((column) =>
      column.tasks.filter(
        (task) =>
          selected_ids.has(task.id) &&
          task_matches_search(task, search_text, search_task_ids),
      ),
    ),
  );
  const selected_task_ids = $derived(selected_tasks.map((task) => task.id));
  const selecting = $derived(selected_task_ids.length > 0);

  $effect(() => {
    void board.active_board_id;
    if (read_only) {
      clear_selection();
    }
    return clear_selection;
  });

  function clear_selection() {
    selected_ids = new Set();
    selection_anchor = null;
  }

  function visible_column_task_ids(column_idx: number) {
    return (board.columns[column_idx]?.tasks ?? [])
      .filter((task) => task_matches_search(task, search_text, search_task_ids))
      .map((task) => task.id);
  }

  function select_task(
    column_idx: number,
    task_id: string,
    event: MouseEvent | null,
  ) {
    const next = new Set(selected_task_ids);
    const column_task_ids = visible_column_task_ids(column_idx);
    const anchor_idx =
      event?.shiftKey && selection_anchor
        ? column_task_ids.indexOf(selection_anchor)
        : -1;
    if (anchor_idx !== -1) {
      const task_idx = column_task_ids.indexOf(task_id);
      for (const id of column_task_ids.slice(
        Math.min(anchor_idx, task_idx),
        Math.max(anchor_idx, task_idx) + 1,
      )) {
        next.add(id);
      }
    } else if (!next.delete(task_id)) {
      next.add(task_id);
    }
    selection_anchor = task_id;
    selected_ids = next;
  }

  function select_column_tasks(column_idx: number) {
    selected_ids = new Set([
      ...selected_task_ids,
      ...visible_column_task_ids(column_idx),
    ]);
  }

  function run_selection_action(action: (task_ids: string[]) => unknown) {
    const task_ids = selected_task_ids;
    clear_selection();
    void action(task_ids);
  }

  function handle_window_keydown(event: KeyboardEvent) {
    if (
      event.key === "Escape" &&
      selecting &&
      !event.defaultPrevented &&
      !document.querySelector(
        '[role="dialog"], [role="alertdialog"], [role="menu"]',
      )
    ) {
      clear_selection();
    }
  }

  function find_task_position(task_id: string): TaskPosition | undefined {
    for (let column_idx = 0; column_idx < board.columns.length; column_idx++) {
      const task_idx = board.columns[column_idx].tasks.findIndex(
        (task) => task.id === task_id,
      );
      if (task_idx !== -1) {
        return { column_idx, task_idx };
      }
    }
    return undefined;
  }

  function handle_drag_start(event: any) {
    if (read_only) {
      return;
    }
    const source = event.operation?.source;
    const source_id = source?.id?.toString();
    if (!source_id) {
      return;
    }

    if (source.type === "item") {
      const origin = find_task_position(source_id);
      dragged_task = origin ? { id: source_id, origin } : null;
      dragged_column = null;
    } else if (source.type === "column") {
      const origin_idx = board.columns.findIndex(
        (column) => column.id === source_id,
      );
      dragged_column = origin_idx === -1 ? null : { id: source_id, origin_idx };
      dragged_task = null;
    }
  }

  function handle_drag_end(event: any) {
    if (read_only) {
      return;
    }
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
        selected_task_ids.length > 1 &&
        selected_task_ids.includes(dragged_task.id)
      ) {
        // Dropping one selected card brings the rest of the selection along,
        // in board order, around the dragged card.
        const column = board.columns[final_position.column_idx];
        const before_task_id =
          column.tasks
            .slice(final_position.task_idx + 1)
            .find((task) => !selected_ids.has(task.id))?.id ?? null;
        void board.move_tasks(selected_task_ids, column.id, before_task_id);
      } else if (
        final_position &&
        (final_position.column_idx !== dragged_task.origin.column_idx ||
          final_position.task_idx !== dragged_task.origin.task_idx)
      ) {
        board.persist_task_move(
          dragged_task.id,
          board.columns[final_position.column_idx].id,
          board.columns[final_position.column_idx].tasks[
            final_position.task_idx + 1
          ]?.id ?? null,
        );
      }
    } else if (source?.type === "column" && dragged_column) {
      const final_column_idx = board.columns.findIndex(
        (column) => column.id === dragged_column?.id,
      );
      if (
        final_column_idx !== -1 &&
        final_column_idx !== dragged_column.origin_idx
      ) {
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
    if (read_only) {
      return;
    }
    const { source, target } = event.operation;
    const source_id = source?.id?.toString();
    const target_id = target?.id?.toString();
    if (!source_id || !target_id || target_id.startsWith("empty_")) {
      return;
    }

    if (source?.type === "column") {
      const from_col_idx = board.columns.findIndex(
        (column) => column.id === source_id,
      );
      const to_col_idx = board.columns.findIndex(
        (column) => column.id === target_id,
      );
      if (
        from_col_idx !== -1 &&
        to_col_idx !== -1 &&
        from_col_idx !== to_col_idx
      ) {
        board.move_column(from_col_idx, to_col_idx, false);
      }
    } else if (source?.type === "item") {
      const from_position = find_task_position(source_id);
      const to_position = find_task_position(target_id);
      if (
        from_position &&
        to_position &&
        from_position.column_idx === to_position.column_idx &&
        (board.columns[from_position.column_idx].sort_order !== "custom" ||
          !!board.columns[from_position.column_idx].tasks[
            from_position.task_idx
          ].pinned !==
            !!board.columns[to_position.column_idx].tasks[to_position.task_idx]
              .pinned)
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

  // ponytail: distance-only activation keeps clicks reliable; add a long-press sensor if needed.
  const dnd_sensors = [
    PointerSensor.configure({
      activationConstraints: () => [distance_activation_constraint(8)] as never,
    }),
    KeyboardSensor,
  ];
</script>

<svelte:window onkeydown={handle_window_keydown} />

{#if board.columns.length === 0}
  <Empty.Root>
    <Empty.Header>
      <Empty.Media variant="icon">
        <SquirrelIcon />
      </Empty.Media>
      <Empty.Title>{m.board_columns_empty()}</Empty.Title>
      <Empty.Description>
        {m.board_columns_empty_description()}
      </Empty.Description>
    </Empty.Header>
    <Empty.Content>
      {#if !read_only}
        <Button onclick={onAddColumn}>{m.column_create()}</Button>
      {/if}
    </Empty.Content>
  </Empty.Root>
{:else}
  <DragDropProvider
    sensors={dnd_sensors}
    onDragStart={handle_drag_start}
    onDragEnd={handle_drag_end}
    onDragOver={handle_drag_over}
  >
    {#if search_text.trim() && search_task_ids?.size === 0}
      <p class="mb-4 text-sm text-muted-foreground" role="status">
        {m.board_search_empty({ query: search_text.trim() })}
      </p>
    {/if}
    <div class="flex min-h-full flex-row space-x-4">
      {#each board.columns as column, column_idx (column.id)}
        <CardColumn
          index={column_idx}
          {column}
          numberTasks={column.tasks.length}
          onDeleteColumn={() => board.delete_column(column.id)}
          onUpdateColumnName={(name) => board.update_column(column.id, name)}
          onUpdateSort={(sort) =>
            board.update_column(column.id, column.name, sort)}
          onAddTask={() => onAddTask(column_idx)}
          onImportTask={() => onImportTask(column.id)}
          onArchiveAllTasks={() => board.archive_all_tasks(column.id)}
          onSelectAllTasks={read_only
            ? undefined
            : () => select_column_tasks(column_idx)}
          {read_only}
        >
          {#each column.tasks as task, task_idx (task.id)}
            {#if task_matches_search(task, search_text, search_task_ids)}
              <CardItem
                index={task_idx}
                {task}
                group={column.id}
                data={{ group: column.id }}
                expand_content={task_expand_mode}
                content_open={board.task_expansion.is_expanded(
                  board.active_board_id,
                  task.id,
                )}
                onToggleContent={() =>
                  board.task_expansion.toggle(board.active_board_id, task.id)}
                onViewTask={() => onViewTask(task, column_idx, task_idx)}
                onEditTask={() => onEditTask(column_idx, task_idx)}
                onDuplicateTask={() => board.duplicate_task(task.id)}
                onSaveAsTemplate={() => onSaveAsTemplate(task)}
                onExportTask={() => onExportTask(task)}
                onDeleteTask={() => board.delete_task(task.id)}
                onArchiveTask={() => board.archive_task(task.id)}
                onFloatTask={() => void board.float_task(task.id)}
                onLoadFlowTabs={() => board.get_flow_tabs()}
                onAddToFlow={(tab) => void board.add_task_to_flow(task.id, tab)}
                selected={selected_ids.has(task.id)}
                {selecting}
                onSelect={read_only
                  ? undefined
                  : (event) => select_task(column_idx, task.id, event)}
                onTogglePin={() =>
                  void board.update_task(task.id, {
                    ...task,
                    pinned: !task.pinned,
                  })}
                {read_only}
              />
            {/if}
          {/each}
        </CardColumn>
      {/each}
    </div>
  </DragDropProvider>
  {#if selecting && !read_only}
    <TaskSelectionBar
      task_ids={selected_task_ids}
      all_pinned={selected_tasks.every((task) => task.pinned)}
      columns={board.columns}
      onPin={(pinned) => void board.set_tasks_pinned(selected_task_ids, pinned)}
      onLoadFlowTabs={() => board.get_flow_tabs()}
      onAddToFlow={(tab) =>
        void board.add_tasks_to_flow(selected_task_ids, tab)}
      onMove={(column_id) =>
        run_selection_action((task_ids) =>
          board.move_tasks(task_ids, column_id),
        )}
      onArchive={() =>
        run_selection_action((task_ids) => board.archive_tasks(task_ids))}
      onDelete={() =>
        run_selection_action((task_ids) => board.delete_tasks(task_ids))}
      onClear={clear_selection}
    />
  {/if}
{/if}
