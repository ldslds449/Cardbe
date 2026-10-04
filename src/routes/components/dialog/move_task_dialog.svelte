<script lang="ts">
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
  let error = $state("");
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
    error = "";
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
          if (!cancelled) {
            error = String(e);
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
      <Dialog.Title>Move Task to Board</Dialog.Title>
      <Dialog.Description
        >Move “{task.title}” to another editable board. Moving clears this
        board’s undo history and cannot be undone.</Dialog.Description
      >
    </Dialog.Header>
    <div class="space-y-2">
      <p class="text-sm font-medium" id="move-board-label">Board</p>
      <Select.Root type="single" bind:value={target} disabled={moving}>
        <Select.Trigger aria-labelledby="move-board-label" class="w-full"
          >{targets.find((b) => String(b.id) === target)?.name ??
            "Choose a board"}</Select.Trigger
        >
        <Select.Content
          >{#each targets as item (item.id)}<Select.Item
              value={String(item.id)}
              label={item.name}>{item.name}</Select.Item
            >{/each}</Select.Content
        >
      </Select.Root>
      {#if targets.length === 0}<p class="text-sm text-muted-foreground">
          No other editable boards available.
        </p>{/if}
    </div>
    <div class="space-y-2">
      <p class="text-sm font-medium" id="move-column-label">Column</p>
      <Select.Root
        type="single"
        bind:value={column}
        disabled={moving || loading || !columns.length}
      >
        <Select.Trigger aria-labelledby="move-column-label" class="w-full"
          >{columns.find((c) => String(c.id) === column)?.name ??
            (loading ? "Loading columns…" : "Choose a column")}</Select.Trigger
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
          Add a column to this board before moving a task.
        </p>{/if}
    </div>
    {#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
    <Dialog.Footer>
      <Button
        variant="outline"
        disabled={moving}
        onclick={() => {
          open = false;
        }}>Cancel</Button
      >
      <Button
        disabled={moving ||
          loading ||
          !column ||
          board.active_board_id !== source_id ||
          !targets.some((b) => String(b.id) === target)}
        onclick={move}>{moving ? "Moving…" : "Move Task"}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
