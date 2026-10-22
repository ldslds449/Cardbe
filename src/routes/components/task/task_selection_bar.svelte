<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatNumber } from "$lib/i18n";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import ColumnsIcon from "@lucide/svelte/icons/columns-3";
  import GaugeIcon from "@lucide/svelte/icons/gauge";
  import MoveRightIcon from "@lucide/svelte/icons/move-right";
  import PinIcon from "@lucide/svelte/icons/pin";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import WorkflowIcon from "@lucide/svelte/icons/workflow";
  import XIcon from "@lucide/svelte/icons/x";
  import MoveTaskDialog from "../dialog/move_task_dialog.svelte";

  import type { Column } from "../../type/column.svelte";
  import { TASK_PRIORITIES, type TaskPriority } from "../../type/task.svelte";
  import PriorityIndicator from "./priority_indicator.svelte";
  import { tab_label, type FlowTab } from "../flow/flow";

  interface TaskSelectionBarProps {
    task_ids: string[];
    all_pinned: boolean;
    columns: Column[];
    onMove: (column_id: string) => void;
    onPin: (pinned: boolean) => void;
    onSetPriority: (priority: TaskPriority | undefined) => void;
    onLoadFlowTabs: () => Promise<FlowTab[] | null>;
    onAddToFlow: (tab: FlowTab) => void;
    onArchive: () => void;
    onDelete: () => void;
    onClear: () => void;
  }

  let {
    task_ids,
    all_pinned,
    columns,
    onMove,
    onPin,
    onSetPriority,
    onLoadFlowTabs,
    onAddToFlow,
    onArchive,
    onDelete,
    onClear,
  }: TaskSelectionBarProps = $props();

  let delete_confirm_open = $state(false);
  let move_board_open = $state(false);
  let flow_tabs = $state.raw<FlowTab[] | null>(null);
  const count = $derived(task_ids.length);
  const pin_label = $derived(
    all_pinned ? m.task_selection_unpin() : m.task_selection_pin(),
  );

  async function load_flow_tabs(open: boolean) {
    if (open) {
      flow_tabs = null;
      flow_tabs = await onLoadFlowTabs();
    }
  }
</script>

<!-- Icon-only actions keep the bar short; names live in labels and tooltips. -->
<div
  role="toolbar"
  aria-label={m.task_selection_toolbar()}
  class="fixed bottom-6 left-1/2 z-40 flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-1 rounded-full border bg-popover p-1.5 text-popover-foreground shadow-lg"
>
  <Button
    variant="ghost"
    size="icon-sm"
    class="rounded-full"
    aria-label={m.task_selection_clear()}
    title={m.task_selection_clear()}
    onclick={onClear}
  >
    <XIcon />
  </Button>
  <span
    class="flex items-center gap-1.5 pr-1 text-sm whitespace-nowrap"
    role="status"
    aria-label={m.task_selection_count({ count })}
    title={m.task_selection_hint()}
  >
    <span
      class="min-w-5 rounded-full bg-primary px-1.5 text-center text-xs leading-5 font-medium text-primary-foreground tabular-nums"
    >
      {formatNumber(count)}
    </span>
    <span class="text-muted-foreground">{m.task_selection_selected()}</span>
  </span>
  <Separator
    orientation="vertical"
    class="mx-1 data-[orientation=vertical]:h-5"
  />

  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon-sm"
          class="rounded-full"
          aria-label={m.task_selection_move()}
          title={m.task_selection_move()}
        >
          <MoveRightIcon />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content side="top" class="max-w-64 min-w-44">
      <DropdownMenu.Label>{m.task_selection_move()}</DropdownMenu.Label>
      {#each columns as column (column.id)}
        <DropdownMenu.Item onSelect={() => onMove(column.id)}>
          <ColumnsIcon />
          <span class="truncate">{column.name}</span>
        </DropdownMenu.Item>
      {/each}
      <DropdownMenu.Separator />
      <DropdownMenu.Item onSelect={() => (move_board_open = true)}>
        <MoveRightIcon />
        {m.task_move_board()}
      </DropdownMenu.Item>
    </DropdownMenu.Content>
  </DropdownMenu.Root>

  <DropdownMenu.Root onOpenChange={load_flow_tabs}>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon-sm"
          class="rounded-full"
          aria-label={m.task_add_to_flow()}
          title={m.task_add_to_flow()}
        >
          <WorkflowIcon />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content side="top" class="max-w-64 min-w-44">
      <DropdownMenu.Label>{m.task_add_to_flow()}</DropdownMenu.Label>
      {#if flow_tabs === null}
        <DropdownMenu.Item disabled>{m.ui_loading()}</DropdownMenu.Item>
      {:else}
        {#each flow_tabs as tab (tab.id)}
          <DropdownMenu.Item onSelect={() => onAddToFlow(tab)}>
            <WorkflowIcon />
            <span class="truncate">{tab_label(tab)}</span>
          </DropdownMenu.Item>
        {/each}
      {/if}
    </DropdownMenu.Content>
  </DropdownMenu.Root>

  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon-sm"
          class="rounded-full"
          aria-label={m.task_priority()}
          title={m.task_priority()}
        >
          <GaugeIcon />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content side="top" class="min-w-36">
      <DropdownMenu.Label>{m.task_priority()}</DropdownMenu.Label>
      {#each [...TASK_PRIORITIES].reverse() as priority (priority)}
        <DropdownMenu.Item onSelect={() => onSetPriority(priority)}>
          <PriorityIndicator {priority} />
        </DropdownMenu.Item>
      {/each}
      <DropdownMenu.Item onSelect={() => onSetPriority(undefined)}>
        {m.task_priority_none()}
      </DropdownMenu.Item>
    </DropdownMenu.Content>
  </DropdownMenu.Root>

  <Button
    variant="ghost"
    size="icon-sm"
    class="rounded-full"
    aria-label={pin_label}
    title={pin_label}
    onclick={() => onPin(!all_pinned)}
  >
    <PinIcon class={all_pinned ? "fill-current" : "rotate-45"} />
  </Button>
  <Button
    variant="ghost"
    size="icon-sm"
    class="rounded-full"
    aria-label={m.task_archive_action()}
    title={m.task_archive_action()}
    onclick={onArchive}
  >
    <ArchiveIcon />
  </Button>
  <Button
    variant="ghost"
    size="icon-sm"
    class="rounded-full text-destructive hover:bg-destructive/10 hover:text-destructive"
    aria-label={m.common_delete()}
    title={m.common_delete()}
    onclick={() => (delete_confirm_open = true)}
  >
    <TrashIcon />
  </Button>
</div>

<AlertDialog.Root bind:open={delete_confirm_open}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{m.task_delete()}</AlertDialog.Title>
      <AlertDialog.Description>
        {m.task_delete_selected_confirm({ count })}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{m.common_cancel()}</AlertDialog.Cancel>
      <AlertDialog.Action
        variant="destructive"
        onclick={() => {
          delete_confirm_open = false;
          onDelete();
        }}>{m.common_delete()}</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>

{#if move_board_open}
  <MoveTaskDialog bind:open={move_board_open} {task_ids} onMoved={onClear} />
{/if}
