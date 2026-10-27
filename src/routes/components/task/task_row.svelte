<script lang="ts" module>
  import type { BoardRole } from "../../board.svelte";

  export interface TaskRowBoard {
    id: number;
    shared_role: BoardRole;
    is_shared?: boolean;
  }
</script>

<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatNumber, getLocale } from "#lib/i18n/index.js";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { Spinner } from "#lib/components/ui/spinner/index.js";
  import { cn } from "#lib/utils.js";

  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import HourglassIcon from "@lucide/svelte/icons/hourglass";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";

  import type { AllTaskItem } from "../../board-base.svelte";
  import { highlight_parts } from "../../utils/task-explorer";
  import { display_task_color } from "../../utils/task-color";
  import LabelBadge from "../label_badge.svelte";
  import DueBadge from "./due_badge.svelte";
  import PriorityIndicator from "./priority_indicator.svelte";
  import { task_item_key, type TaskActions } from "./task-actions.svelte";

  // Rows show a few labels; the task details list them all.
  const MAX_LABELS = 3;

  let {
    item,
    density,
    board,
    now,
    highlight = "",
    actions,
    onOpenTask,
    onMenu,
  }: {
    item: AllTaskItem;
    /** Compact rows keep a fixed height for virtual lists. */
    density: "compact" | "full";
    board: TaskRowBoard | undefined;
    now: Date;
    /** Search text to mark in the title. */
    highlight?: string;
    actions: TaskActions;
    onOpenTask: (item: AllTaskItem) => unknown;
    /** Opens the list's actions menu for this task below `anchor`. */
    onMenu: (item: AllTaskItem, anchor: HTMLElement) => void;
  } = $props();

  const writable = $derived(Boolean(board && board.shared_role !== "viewer"));
  const shared = $derived(
    Boolean(board?.is_shared || board?.shared_role === "editor"),
  );
  const busy = $derived(actions.is_busy(item));
  const archived = $derived(Boolean(item.archived_at));
  const finished = $derived(item.done || archived);
  const title = $derived(item.task.title || m.task_untitled());
  const color = $derived(display_task_color(item.task.color));
  const location = $derived(
    m.focus_location({
      board: item.board_name,
      column: archived
        ? m.explorer_archived()
        : (item.column_name ?? m.explorer_no_column()),
    }),
  );
  const reopen_label = $derived(
    archived ? m.explorer_unarchive() : m.task_reopen_action(),
  );

  function format_archive_time(archived_at: Date): string {
    return m.explorer_archived_date({
      date: archived_at.toLocaleDateString(getLocale(), {
        month: "short",
        day: "numeric",
        year:
          archived_at.getFullYear() === now.getFullYear()
            ? undefined
            : "numeric",
      }),
    });
  }

  function open() {
    void actions.run(item, async () => onOpenTask(item), false);
  }
</script>

{#snippet marked(text: string)}
  {#each highlight_parts(text, highlight) as part, index (index)}
    {#if part.match}<mark class="rounded-sm bg-primary/20 text-inherit"
        >{part.text}</mark
      >{:else}{part.text}{/if}
  {/each}
{/snippet}

<div
  class={cn(
    "relative flex items-center gap-2 overflow-hidden px-3",
    density === "compact"
      ? "h-full border-b border-border/60 transition-colors hover:bg-muted/50"
      : "rounded-lg border bg-card py-2 shadow-xs",
  )}
  data-task-key={task_item_key(item)}
>
  {#if color}
    <!-- The task's color, like the edge of a card on the board. -->
    <span
      class={cn(
        "absolute start-0 w-1",
        density === "compact" ? "inset-y-2 rounded-e-full" : "inset-y-0",
      )}
      style:background-color={color}
      aria-hidden="true"
    ></span>
  {/if}
  {#if writable}
    <Button
      variant="ghost"
      size="icon-sm"
      class={cn(
        "shrink-0",
        finished ? "text-primary" : "text-muted-foreground",
      )}
      disabled={busy}
      aria-label={finished ? reopen_label : m.task_complete()}
      title={finished ? reopen_label : m.task_complete()}
      onclick={() =>
        finished ? actions.ask_to_reopen(item) : actions.complete(item)}
    >
      {#if finished}
        <RotateCcwIcon />
      {:else}
        <CircleCheckIcon />
      {/if}
    </Button>
  {/if}
  <button
    type="button"
    class="min-w-0 flex-1 self-stretch rounded-sm py-1 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
    data-task-open="true"
    aria-label={m.explorer_view_named({ name: title })}
    onclick={open}
  >
    <span class="flex min-w-0 items-center gap-2">
      <span
        class={cn(
          "block min-w-0 truncate text-sm",
          density === "compact" && "font-medium",
          finished && "text-muted-foreground line-through",
        )}
      >
        {#if item.task.title}
          {@render marked(item.task.title)}
        {:else}
          {m.task_untitled()}
        {/if}
      </span>
    </span>
    <span
      class={cn(
        "mt-1 flex min-w-0 items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground",
        density === "compact" ? "overflow-hidden" : "flex-wrap",
      )}
    >
      <span class="min-w-0 truncate">{location}</span>
      {#if item.archived_at}
        <span class="inline-flex shrink-0 items-center gap-1">
          <ArchiveIcon class="size-3.5" />
          {format_archive_time(item.archived_at)}
        </span>
      {:else if item.task.due_time}
        <DueBadge
          due_time={item.task.due_time}
          {now}
          at_midnight={item.task.due_at_midnight}
        />
      {/if}
      {#if item.task.priority}
        <PriorityIndicator priority={item.task.priority} class="shrink-0" />
      {/if}
      {#if item.waiting_count}
        <span
          class="inline-flex shrink-0 items-center gap-1"
          title={m.task_waiting_count({ count: item.waiting_count })}
        >
          <HourglassIcon class="size-3.5" />
          <span aria-hidden="true">{formatNumber(item.waiting_count)}</span>
          <span class="sr-only"
            >{m.task_waiting_count({ count: item.waiting_count })}</span
          >
        </span>
      {/if}
      {#if item.task.recurrence && !archived}
        <Repeat2Icon
          class="size-3.5 shrink-0"
          aria-label={m.explorer_recurring()}
        />
      {/if}
      {#if !writable}
        <Badge variant="outline" class="shrink-0 py-0 text-[11px]"
          >{m.board_read_only()}</Badge
        >
      {:else if shared}
        <Badge variant="outline" class="shrink-0 py-0 text-[11px]"
          >{m.explorer_shared()}</Badge
        >
      {/if}
      {#each item.task.labels.slice(0, MAX_LABELS) as label (label)}
        <LabelBadge {label} class="shrink-0 py-0 text-[11px]" />
      {/each}
      {#if item.task.labels.length > MAX_LABELS}
        <span class="shrink-0"
          >+{formatNumber(item.task.labels.length - MAX_LABELS)}</span
        >
      {/if}
    </span>
    {#if item.excerpt}
      <!-- Shows why a search found the task when its title does not. -->
      <span
        class="mt-0.5 block truncate text-xs leading-4 text-muted-foreground"
        >{@render marked(item.excerpt)}</span
      >
    {/if}
  </button>
  {#if busy}
    <Spinner class="busy-indicator size-4 shrink-0" />
  {/if}
  <Button
    variant="ghost"
    size="icon-sm"
    class="shrink-0"
    aria-haspopup="menu"
    aria-label={m.focus_task_actions({ name: title })}
    title={m.task_menu()}
    onclick={(event) => onMenu(item, event.currentTarget)}
  >
    <EllipsisIcon />
  </Button>
</div>

<style>
  /* Quick actions finish before the spinner would only flash. */
  :global(.busy-indicator) {
    animation: show-busy 200ms step-end both;
  }

  @keyframes show-busy {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
