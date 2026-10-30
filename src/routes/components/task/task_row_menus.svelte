<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import type { Snippet } from "svelte";

  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import GaugeIcon from "@lucide/svelte/icons/gauge";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import ListTodoIcon from "@lucide/svelte/icons/list-todo";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
  import SunIcon from "@lucide/svelte/icons/sun";
  import Undo2Icon from "@lucide/svelte/icons/undo-2";

  import type { AllTaskItem } from "../../board-base.svelte";
  import { is_task_priority, TASK_PRIORITIES } from "../../type/task.svelte";
  import ChooseColumnDialog from "../dialog/choose_column_dialog.svelte";
  import PriorityIndicator from "./priority_indicator.svelte";
  import { task_item_key, type TaskActions } from "./task-actions.svelte";
  import { move_row_focus } from "./task-row-keys";
  import type { TaskRowBoard } from "./task_row.svelte";

  let {
    items,
    boards,
    actions,
    class: className,
    children,
    onOpenTask,
    onEditTask,
    onArchiveTask,
    onOpenBoard,
  }: {
    /** The listed tasks; rows name theirs with `data-task-key`. */
    items: AllTaskItem[];
    boards: Map<number, TaskRowBoard>;
    actions: TaskActions;
    class?: string;
    children: Snippet;
    onOpenTask: (item: AllTaskItem) => unknown;
    onEditTask: (item: AllTaskItem) => unknown;
    onArchiveTask: (item: AllTaskItem) => unknown;
    onOpenBoard: (board_id: number) => void;
  } = $props();

  // One menu of each kind serves every row, so a long list does not build a
  // menu per task.
  let context_open = $state(false);
  let context_item = $state<AllTaskItem | null>(null);
  let menu_open = $state(false);
  let menu_item = $state<AllTaskItem | null>(null);
  let menu_anchor = $state<HTMLElement | null>(null);
  // Pressing the open menu's button closes the menu before its click lands;
  // that click must not open it again.
  let closed_from: { anchor: Element; at: number } | null = null;

  /** Opens the actions menu of `item` below `anchor`, its row's menu button. */
  export function open_menu(item: AllTaskItem, anchor: HTMLElement) {
    const closed = closed_from;
    closed_from = null;
    if (closed?.anchor === anchor && performance.now() - closed.at < 1000) {
      return;
    }
    menu_item = item;
    menu_anchor = anchor;
    menu_open = true;
  }

  function find_item(target: EventTarget | null): AllTaskItem | null {
    const key =
      target instanceof Element
        ? target.closest<HTMLElement>("[data-task-key]")?.dataset.taskKey
        : undefined;
    return (key && items.find((item) => task_item_key(item) === key)) || null;
  }

  // Right-clicking between rows keeps the system menu.
  function pick_context_item(event: MouseEvent) {
    context_item = find_item(event.target);
    if (!context_item) {
      event.stopPropagation();
    }
  }

  function open_task(item: AllTaskItem) {
    void actions.run(item, async () => onOpenTask(item), false);
  }
</script>

{#snippet menu_items(
  menu: typeof DropdownMenu | typeof ContextMenu,
  item: AllTaskItem,
)}
  {@const board = boards.get(item.board_id)}
  {@const writable = Boolean(board && board.shared_role !== "viewer")}
  {@const busy = actions.is_busy(item)}
  {@const archived = Boolean(item.archived_at)}
  <menu.Item disabled={busy} onclick={() => open_task(item)}>
    <ListTodoIcon />{m.explorer_view()}
  </menu.Item>
  {#if !archived}
    <menu.Item disabled={busy} onclick={() => actions.add_to_plan(item)}>
      <SunIcon />{m.daily_plan_add()}
    </menu.Item>
  {/if}
  {#if writable}
    {#if !archived}
      <menu.Item
        disabled={busy}
        onclick={() =>
          void actions.run(item, async () => onEditTask(item), false)}
      >
        <PencilIcon />{m.explorer_edit()}
      </menu.Item>
    {/if}
    <menu.Sub>
      <menu.SubTrigger disabled={busy}>
        <GaugeIcon />
        {m.task_priority()}
      </menu.SubTrigger>
      <menu.SubContent class="min-w-36">
        <menu.RadioGroup
          value={item.task.priority ?? "none"}
          onValueChange={(value) =>
            actions.set_priority(
              item,
              is_task_priority(value) ? value : undefined,
            )}
        >
          {#each [...TASK_PRIORITIES].reverse() as priority (priority)}
            <menu.RadioItem value={priority}>
              <PriorityIndicator {priority} />
            </menu.RadioItem>
          {/each}
          <menu.RadioItem value="none">{m.task_priority_none()}</menu.RadioItem>
        </menu.RadioGroup>
      </menu.SubContent>
    </menu.Sub>
    <menu.Separator />
    {#if archived}
      <menu.Item disabled={busy} onclick={() => actions.ask_to_reopen(item)}>
        <Undo2Icon />{m.explorer_unarchive()}
      </menu.Item>
    {:else if item.done}
      <menu.Item disabled={busy} onclick={() => actions.ask_to_reopen(item)}>
        <RotateCcwIcon />{m.task_reopen_action()}
      </menu.Item>
    {:else}
      <menu.Item disabled={busy} onclick={() => actions.complete(item)}>
        <CircleCheckIcon />{m.task_complete()}
      </menu.Item>
    {/if}
    {#if !archived}
      <menu.Item
        disabled={busy}
        onclick={() =>
          void actions.run(item, async () => onArchiveTask(item), false)}
      >
        <ArchiveIcon />{m.explorer_archive()}
      </menu.Item>
    {/if}
  {/if}
  <menu.Separator />
  <menu.Item onclick={() => onOpenBoard(item.board_id)}>
    <LayoutDashboardIcon />
    {m.focus_open_board()}
  </menu.Item>
{/snippet}

<ContextMenu.Root bind:open={context_open}>
  <ContextMenu.Trigger
    class={className}
    oncontextmenucapture={pick_context_item}
    onkeydown={move_row_focus}
  >
    {@render children()}
  </ContextMenu.Trigger>
  <ContextMenu.Content class="min-w-44">
    {#if context_item}
      {@render menu_items(ContextMenu, context_item)}
    {/if}
  </ContextMenu.Content>
</ContextMenu.Root>

<DropdownMenu.Root bind:open={menu_open}>
  <DropdownMenu.Content
    align="end"
    class="min-w-44"
    customAnchor={menu_anchor}
    onInteractOutside={(event) => {
      if (
        menu_anchor &&
        event.target instanceof Node &&
        menu_anchor.contains(event.target)
      ) {
        closed_from = { anchor: menu_anchor, at: performance.now() };
      }
    }}
    onCloseAutoFocus={(event) => {
      // Without a trigger, focus returns to the row's menu button.
      event.preventDefault();
      menu_anchor?.focus();
    }}
  >
    {#if menu_item}
      {@render menu_items(DropdownMenu, menu_item)}
    {/if}
  </DropdownMenu.Content>
</DropdownMenu.Root>

<ChooseColumnDialog
  bind:open={actions.reopen_open}
  title={actions.reopen_item?.archived_at
    ? m.explorer_unarchive()
    : m.task_reopen()}
  description={m.task_reopen_description({
    name: actions.reopen_item?.task.title || m.task_untitled(),
  })}
  confirm_label={actions.reopen_item?.archived_at
    ? m.explorer_unarchive()
    : m.task_reopen_action()}
  columns={actions.reopen_columns}
  onConfirm={(column_id) => actions.reopen(column_id)}
/>
