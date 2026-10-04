<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "$lib/command-errors";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { board } from "../../board.svelte";
  import type { Task } from "../../type/task.svelte";
  import type { ColumnSerialized } from "../../type/column.svelte";

  let { open = $bindable(false), task }: { open: boolean; task: Task } =
    $props();
  let target = $state("");
  let column = $state("");
  let columns = $state<ColumnSerialized[]>([]);
  let loading = $state(false);
  let moving = $state(false);
  let error = $state<CommandError | null>(null);
  const source_id = board.active_board_id;
  const targets = $derived(
    board.boards.filter(
      (b) => b.id !== source_id && b.shared_role !== "viewer",
    ),
  );

  $effect(() => {
    const id = target;
    let cancelled = false;
    columns = [];
    column = "";
    error = null;
    loading = Boolean(id);
    if (id) {
      invoke<ColumnSerialized[]>("get_board_columns", { boardId: Number(id) })
        .then((result) => {
          if (!cancelled) {
            columns = result;
            column = result[0] ? String(result[0].id) : "";
          }
        })
        .catch((e) => {
          logger.error("task.move_columns_load.failed", e);
          if (!cancelled) {
            error = parseCommandError(e) ?? { code: "INTERNAL_ERROR" };
          }
        })
        .finally(() => {
          if (!cancelled) {
            loading = false;
          }
        });
    }
    return () => {
      cancelled = true;
    };
  });

  async function move() {
    if (
      moving ||
      board.active_board_id !== source_id ||
      !targets.some((b) => String(b.id) === target) ||
      !column
    ) {
      return;
    }
    moving = true;
    if (
      await board.move_task_to_board(task.id, Number(target), Number(column))
    ) {
      open = false;
    }
    moving = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{m.task_move_board_title()}</Dialog.Title>
      <Dialog.Description
        >{m.task_move_board_description({
          name: task.title,
        })}</Dialog.Description
      >
    </Dialog.Header>
    <div class="space-y-2">
      <p class="text-sm font-medium" id="move-board-label">{m.board_title()}</p>
      <Select.Root type="single" bind:value={target} disabled={moving}>
        <Select.Trigger aria-labelledby="move-board-label" class="w-full"
          >{targets.find((b) => String(b.id) === target)?.name ??
            m.ui_choose_a_board()}</Select.Trigger
        >
        <Select.Content
          >{#each targets as item (item.id)}<Select.Item
              value={String(item.id)}
              label={item.name}>{item.name}</Select.Item
            >{/each}</Select.Content
        >
      </Select.Root>
      {#if targets.length === 0}<p class="text-sm text-muted-foreground">
          {m.task_move_board_empty()}
        </p>{/if}
    </div>
    <div class="space-y-2">
      <p class="text-sm font-medium" id="move-column-label">
        {m.explorer_column()}
      </p>
      <Select.Root
        type="single"
        bind:value={column}
        disabled={moving || loading || !columns.length}
      >
        <Select.Trigger aria-labelledby="move-column-label" class="w-full"
          >{columns.find((c) => String(c.id) === column)?.name ??
            (loading
              ? m.task_move_columns_loading()
              : m.archive_choose())}</Select.Trigger
        >
        <Select.Content
          >{#each columns as item (item.id)}<Select.Item
              value={String(item.id)}
              label={item.name}>{item.name}</Select.Item
            >{/each}</Select.Content
        >
      </Select.Root>
      {#if target && !loading && !error && !columns.length}<p
          class="text-sm text-muted-foreground"
        >
          {m.task_move_column_empty()}
        </p>{/if}
    </div>
    {#if error}<p class="text-sm text-destructive" role="alert">
        {translateCommandError(error)}
      </p>{/if}
    <Dialog.Footer>
      <Button
        variant="outline"
        disabled={moving}
        onclick={() => {
          open = false;
        }}>{m.common_cancel()}</Button
      >
      <Button
        disabled={moving ||
          loading ||
          !column ||
          board.active_board_id !== source_id ||
          !targets.some((b) => String(b.id) === target)}
        onclick={move}>{moving ? m.task_moving() : m.task_move_action()}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
