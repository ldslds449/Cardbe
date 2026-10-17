<script lang="ts">
  import TaskActionItems from "../task/task_action_items.svelte";
  import CardSizeMenu from "./card_size_menu.svelte";
  import CardScaleHandle from "./card_scale_handle.svelte";
  import { Handle, Position, useStore, type NodeProps } from "@xyflow/svelte";
  import type { TaskNode } from "./flow";
  import { Badge } from "$lib/components/ui/badge";
  import * as ContextMenu from "$lib/components/ui/context-menu";
  import DeleteTaskDialog from "../task/delete_task_dialog.svelte";
  import ChecklistSummary from "../task/checklist_summary.svelte";
  import { formatDate } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages";
  import XIcon from "@lucide/svelte/icons/x";
  let { id, data, selected }: NodeProps<TaskNode> = $props();
  let delete_open = $state(false);
  let card = $state<HTMLElement | null>(null);
  let preview = $state<number | null>(null);
  const store = useStore();
</script>

<ContextMenu.Root>
  <ContextMenu.Trigger
    bind:ref={card}
    tabindex={0}
    aria-haspopup="menu"
    aria-label={data.task.title}
    ondblclick={() => data.onOpen(data.task)}
    aria-describedby={data.archived
      ? `flow-archive-${data.task.id}`
      : undefined}
    style={`zoom: ${preview ?? data.scale}`}
    class={`block w-64 rounded-lg border p-3 focus-visible:ring-2 focus-visible:ring-ring ${data.archived ? "border-dashed bg-muted/40 text-muted-foreground" : "bg-card text-card-foreground shadow-sm"} ${selected ? "border-primary ring-2 ring-ring/40" : data.archived ? "border-muted-foreground/60" : "border-border"}`}
  >
    {#if data.archived}
      <span id={`flow-archive-${data.task.id}`} class="sr-only"
        >{m.explorer_archived()}</span
      >
    {/if}
    <span class="line-clamp-3 break-words font-medium">{data.task.title}</span>
    <div
      class="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground"
    >
      {#if !data.archived}<Badge variant="secondary" class="max-w-full truncate"
          >{data.column}</Badge
        >{/if}
      {#if data.task.due_time}<span>{formatDate(data.task.due_time)}</span>{/if}
      {#if data.task.items.length > 0}<span
          ><ChecklistSummary items={data.task.items} /></span
        >{/if}
    </div>
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    <TaskActionItems
      context
      onView={() => data.onOpen(data.task)}
      onEdit={!data.locked && !data.archived
        ? () => data.onEdit(data.task)
        : undefined}
      onArchive={!data.locked && !data.archived
        ? () => data.onArchive(data.task)
        : undefined}
      onDelete={!data.locked && !data.archived
        ? () => (delete_open = true)
        : undefined}
    >
      {#snippet afterArchive()}
        <!-- Menus ignore pending saves so they stay steady while stepping sizes. -->
        {#if !data.locked}
          <CardSizeMenu
            scale={data.scale}
            onScale={(change) => data.onScale(data.task.id, change, true)}
          />
          <ContextMenu.Item onclick={() => data.onRemove(data.task.id)}
            ><XIcon />{m.flow_remove_node()}</ContextMenu.Item
          >
        {/if}
      {/snippet}
    </TaskActionItems>
  </ContextMenu.Content>
</ContextMenu.Root>
{#if selected && !data.locked}
  <CardScaleHandle
    {id}
    scale={data.scale}
    target={card}
    onPreview={(scale) => (preview = scale)}
    onCommit={(scale) => data.onScale(data.task.id, () => scale, false)}
  />
{/if}
{#each [Position.Left, Position.Right, Position.Top, Position.Bottom] as position}
  <Handle
    id={position}
    class={`task-connector !size-4 !border-2 ${store.connection.inProgress ? "connecting" : ""}`}
    type="source"
    {position}
    aria-label={m.flow_handle()}
    isConnectable={!data.read_only}
    tabindex={data.read_only ? -1 : 0}
    aria-disabled={data.read_only}
    onkeydown={(event) => {
      if (!data.read_only && (event.key === "Enter" || event.key === " ")) {
        event.preventDefault();
        event.stopPropagation();
        (event.currentTarget as HTMLElement).click();
      }
    }}
  />
{/each}
<DeleteTaskDialog
  bind:open={delete_open}
  task_title={data.task.title}
  onConfirm={() => data.onDelete(data.task)}
/>

<style>
  :global(.task-connector) {
    opacity: 0;
    transition: opacity 120ms;
  }
  :global(.svelte-flow__node:hover .task-connector),
  :global(.svelte-flow__node:focus-visible .task-connector),
  :global(.svelte-flow__node:has(:focus-visible) .task-connector),
  :global(.task-connector.connecting) {
    opacity: 1;
  }
</style>
