<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import { formatDateTime } from "$lib/i18n";
  import { translateCommandError } from "$lib/command-errors";
  import HistoryIcon from "@lucide/svelte/icons/history";
  import CollapsibleBlock from "../collapsible_block.svelte";
  import type { Column } from "../../type/column.svelte";
  import {
    load_task_events,
    task_history,
    type TaskEvent,
  } from "../statistics/task-events";

  let {
    board_id,
    task_id,
    columns,
  }: {
    board_id: number;
    task_id: string;
    columns: Column[];
  } = $props();

  // The structured error is kept so its message follows language changes.
  let result = $state<
    { events: TaskEvent[] } | { error: unknown } | undefined
  >();
  $effect(() => {
    let current = true;
    result = undefined;
    // A task still being created has no persisted id or history yet.
    if (!/^task_\d+$/.test(task_id)) {
      result = { events: [] };
      return;
    }
    load_task_events(board_id, { taskId: task_id })
      .then((events) => {
        if (current) {
          result = { events };
        }
      })
      .catch((error: unknown) => {
        if (current) {
          result = { error };
        }
      });
    return () => (current = false);
  });
  const history = $derived(
    result && "events" in result ? task_history(result.events, task_id) : null,
  );

  function column_name(id: string | null) {
    return (
      columns.find((column) => column.id === id)?.name ??
      m.task_history_deleted_column()
    );
  }

  function describe(event: TaskEvent) {
    const from = column_name(event.from_column_id);
    const to = column_name(event.to_column_id);
    switch (event.kind) {
      case "created":
        return m.task_history_created({ column: to });
      case "moved":
        return m.task_history_moved({ from, to });
      case "archived":
        return m.task_history_archived({ from });
      case "restored":
        return m.task_history_restored({ to });
      case "transferred_in":
        return m.task_history_transferred_in({ to });
      case "transferred_out":
        return m.task_history_transferred_out();
      default:
        return m.task_history_deleted();
    }
  }
</script>

<div>
  <div class="mb-2 flex items-center gap-2 text-sm font-medium">
    <HistoryIcon class="size-4" />
    <span>{m.task_history()}</span>
  </div>
  {#if !result}
    <p class="text-sm text-muted-foreground" role="status">
      {m.task_history_loading()}
    </p>
  {:else if "error" in result}
    <p class="text-sm text-destructive" role="alert">
      {translateCommandError(result.error)}
    </p>
  {:else if !history || history.entries.length === 0}
    <p class="text-sm text-muted-foreground">{m.task_history_empty()}</p>
  {:else}
    <CollapsibleBlock
      max_height={180}
      expand_label={m.markdown_show_all()}
      collapse_label={m.markdown_show_less()}
    >
      <ol class="space-y-2">
        {#each history.entries.toReversed() as event (event.id)}
          <li class="text-sm">
            <p class="[overflow-wrap:anywhere]">{describe(event)}</p>
            <p class="flex flex-wrap gap-x-2 text-xs text-muted-foreground">
              <time datetime={event.at.toISOString()}
                >{formatDateTime(event.at)}</time
              >
              {#if event.origin === "remote"}
                <span>{m.task_history_from_remote()}</span>
              {:else if event.origin === "plugin"}
                <span>{m.task_history_from_plugin()}</span>
              {/if}
            </p>
          </li>
        {/each}
      </ol>
    </CollapsibleBlock>
    {#if !history.complete}
      <p class="mt-2 text-xs text-muted-foreground">
        {m.task_history_incomplete()}
      </p>
    {/if}
  {/if}
</div>
