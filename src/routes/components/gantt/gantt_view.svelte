<script lang="ts">
  import { formatDate, formatNumber } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import ChartGanttIcon from "@lucide/svelte/icons/chart-gantt";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import PlayIcon from "@lucide/svelte/icons/play";
  import Share2Icon from "@lucide/svelte/icons/share-2";
  import SlidersHorizontalIcon from "@lucide/svelte/icons/sliders-horizontal";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import UndoIcon from "@lucide/svelte/icons/undo-2";
  import { onMount, tick, untrack } from "svelte";

  import type { Column } from "../../type/column.svelte";
  import type { Archive } from "../../type/archive.svelte";
  import type { Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";
  import TaskActionItems from "../task/task_action_items.svelte";
  import DeleteTaskDialog from "../task/delete_task_dialog.svelte";
  import UnarchiveTaskDialog from "../archive/unarchive_task_dialog.svelte";
  import {
    calendar_drop_dates,
    start_of_day,
    type CalendarDragMode,
  } from "../calendar/calendar";
  import {
    date_from_day_number,
    day_number,
    is_date_only,
    task_date_range,
    task_dates,
    type TaskDateRange,
    type TaskDates,
  } from "../../utils/task-date-range";
  import {
    build_gantt_rows,
    build_gantt_timeline,
    gantt_header_cells,
    visible_row_range,
    type GanttHeaderCell,
    type GanttRow,
    type GanttScale,
    type GanttTask,
  } from "./gantt";

  const LIST_WIDTH = 256;
  const ROW_HEIGHT = 36;
  const HEADER_ROW_HEIGHT = 28;
  // Pointer travel before a press on a bar becomes a drag instead of a click.
  const DRAG_THRESHOLD = 4;
  // Distance from a visible timeline end where dragging scrolls, and the
  // fastest scroll per frame once the pointer is that far in or beyond.
  const EDGE_SCROLL_ZONE = 48;
  const EDGE_SCROLL_MAX_STEP = 16;
  // Viewport widths of timeline kept beyond each visible end; scrolling
  // closer adds more days so the timeline never runs out.
  const EXTEND_MARGIN_SCREENS = 1;
  const EXTEND_CHUNK_SCREENS = 2;

  interface GanttViewProps {
    columns: Column[];
    archives?: Archive[];
    search_text?: string;
    search_task_ids?: ReadonlySet<string> | null;
    scale?: GanttScale;
    show_archived?: boolean;
    onViewTask?: (task: Task) => void;
    onViewArchivedTask?: (task: Task) => void;
    onEditTask?: (task: Task) => void;
    onDuplicateTask?: (task: Task) => void;
    onSaveAsTemplate?: (task: Task) => void;
    onExportTask?: (task: Task) => void;
    onArchiveTask?: (task: Task) => void;
    onUnarchiveTask?: (column_id: string, task_id: string) => void;
    onDeleteTask?: (task: Task) => void;
    onRescheduleTask?: (task: Task, dates: TaskDates) => void;
    read_only?: boolean;
  }

  let {
    columns,
    archives = [],
    search_text = "",
    search_task_ids = null,
    scale = $bindable("day"),
    show_archived = $bindable(false),
    onViewTask = () => {},
    onViewArchivedTask = () => {},
    onEditTask = () => {},
    onDuplicateTask = () => {},
    onSaveAsTemplate = () => {},
    onExportTask = () => {},
    onArchiveTask = () => {},
    onUnarchiveTask = () => {},
    onDeleteTask = () => {},
    onRescheduleTask = () => {},
    read_only = false,
  }: GanttViewProps = $props();

  let delete_confirm_open = $state(false);
  let delete_target = $state<Task | null>(null);
  let unarchive_dialog_open = $state(false);
  let unarchive_target = $state<Task | null>(null);

  const keyboard_hint_id = `gantt-keyboard-hint-${Math.random().toString(36).slice(2)}`;

  let today = $state(start_of_day(new Date()));
  let today_day = $derived(day_number(today));

  let scroller = $state<HTMLDivElement | null>(null);
  let scroll_top = $state(0);
  let viewport_height = $state(0);
  let viewport_client_width = $state(0);
  // Days added beyond the padding as the timeline scrolls toward either end.
  let extend_before = $state(0);
  let extend_after = $state(0);

  let gantt = $derived(
    build_gantt_rows(
      columns,
      search_text,
      archives,
      show_archived,
      search_task_ids,
    ),
  );
  let timeline = $derived(
    build_gantt_timeline(
      gantt.rows,
      today_day,
      scale,
      Math.max(0, viewport_client_width - LIST_WIDTH),
      extend_before,
      extend_after,
    ),
  );
  let upper_cells = $derived(
    gantt_header_cells(timeline, scale === "month" ? "year" : "month"),
  );
  let lower_cells = $derived(
    gantt_header_cells(
      timeline,
      scale === "day" ? "day" : scale === "week" ? "week" : "month",
    ),
  );
  let window_range = $derived(
    visible_row_range(
      Math.max(0, scroll_top - HEADER_ROW_HEIGHT * 2),
      viewport_height,
      ROW_HEIGHT,
      gantt.rows.length,
    ),
  );
  let visible_rows = $derived(
    gantt.rows
      .slice(window_range.start, window_range.end)
      .map((row, offset) => ({ row, index: window_range.start + offset })),
  );
  let total_width = $derived(LIST_WIDTH + timeline.width);
  let body_height = $derived(gantt.rows.length * ROW_HEIGHT);

  interface ActiveDrag {
    task: Task;
    mode: CalendarDragMode;
    grab_day: number;
    origin_x: number;
    origin_scroll: number;
    pointer_x: number;
    moved: boolean;
  }

  interface DragPreview {
    task_id: string;
    dates: TaskDates | null;
    range: TaskDateRange | null;
  }

  interface ActivePan {
    origin_x: number;
    origin_y: number;
    scroll_left: number;
    scroll_top: number;
  }

  let active_drag: ActiveDrag | null = null;
  let drag_preview = $state<DragPreview | null>(null);
  // A press that turned into a drag must not also open the task.
  let suppress_click = false;
  let edge_scroll_frame = 0;
  let active_pan: ActivePan | null = null;
  let panning = $state(false);
  // The timeline origin the scroll position was last measured against.
  let anchor_first_day: number | null = null;
  let anchor_day_width = 0;

  function refresh_today() {
    const current = start_of_day(new Date());
    if (current.getTime() !== today.getTime()) {
      today = current;
    }
  }

  function viewport_width(): number {
    return Math.max(0, (scroller?.clientWidth ?? 0) - LIST_WIDTH);
  }

  function scroll_to_day(day: number, fraction = 1 / 3) {
    if (!scroller) {
      return;
    }
    scroller.scrollLeft = Math.max(
      0,
      (day - timeline.first_day) * timeline.day_width -
        viewport_width() * fraction,
    );
  }

  function go_to_today() {
    refresh_today();
    scroll_to_day(today_day);
  }

  async function set_scale(next: GanttScale) {
    if (next === scale) {
      return;
    }
    // Keep the day at the center of the timeline in place across scales.
    const center_day = scroller
      ? timeline.first_day +
        (scroller.scrollLeft + viewport_width() / 2) / timeline.day_width
      : today_day;
    scale = next;
    await tick();
    scroll_to_day(center_day, 1 / 2);
  }

  // The ScrollArea viewport scrolls; track it for row windowing and the
  // minimum timeline width.
  $effect(() => {
    const viewport = scroller;
    if (!viewport) {
      return;
    }
    const update_size = () => {
      viewport_height = viewport.clientHeight;
      viewport_client_width = viewport.clientWidth;
      extend_timeline();
    };
    const update_scroll = () => {
      scroll_top = viewport.scrollTop;
      extend_timeline();
    };
    // Measuring reads the timeline, which must not resubscribe this effect.
    untrack(() => {
      update_size();
      update_scroll();
    });
    const observer = new ResizeObserver(update_size);
    observer.observe(viewport);
    viewport.addEventListener("scroll", update_scroll, { passive: true });
    return () => {
      observer.disconnect();
      viewport.removeEventListener("scroll", update_scroll);
    };
  });

  // Adds days at an end once the visible range comes within a margin of it.
  function extend_timeline() {
    const viewport = scroller;
    const width = viewport_width();
    if (!viewport || width === 0) {
      return;
    }
    const margin = width * EXTEND_MARGIN_SCREENS;
    const chunk = Math.ceil(
      (width * EXTEND_CHUNK_SCREENS) / timeline.day_width,
    );
    if (viewport.scrollLeft < margin) {
      extend_before += chunk;
    }
    if (
      viewport.scrollLeft + viewport.clientWidth >
      viewport.scrollWidth - margin
    ) {
      extend_after += chunk;
    }
  }

  // Days added before the timeline shift its content right; scroll by the
  // same amount so the visible dates, and any drag or pan, stay in place.
  $effect(() => {
    const { first_day, day_width } = timeline;
    const viewport = untrack(() => scroller);
    if (
      viewport &&
      anchor_first_day !== null &&
      anchor_day_width === day_width &&
      anchor_first_day !== first_day
    ) {
      const shift = (anchor_first_day - first_day) * day_width;
      viewport.scrollLeft += shift;
      if (active_drag) {
        active_drag.origin_scroll += shift;
      }
      if (active_pan) {
        active_pan.scroll_left += shift;
      }
    }
    anchor_first_day = first_day;
    anchor_day_width = day_width;
  });

  onMount(() => {
    void tick().then(() => scroll_to_day(today_day));

    function handle_visibility_change() {
      if (!document.hidden) {
        refresh_today();
      }
    }
    window.addEventListener("focus", refresh_today);
    document.addEventListener("visibilitychange", handle_visibility_change);
    return () => {
      window.removeEventListener("focus", refresh_today);
      document.removeEventListener(
        "visibilitychange",
        handle_visibility_change,
      );
      stop_drag();
      stop_pan();
    };
  });

  function entry_editable(entry: GanttTask): boolean {
    return !read_only && !entry.archived;
  }

  function entry_color(entry: GanttTask): string {
    return entry.archived
      ? "var(--muted-foreground)"
      : display_task_color(entry.task.color) || "var(--muted-foreground)";
  }

  function ask_to_delete(task: Task) {
    delete_target = task;
    delete_confirm_open = true;
  }

  function confirm_delete() {
    if (delete_target) {
      onDeleteTask(delete_target);
    }
    delete_target = null;
  }

  function ask_to_unarchive(task: Task) {
    unarchive_target = task;
    unarchive_dialog_open = true;
  }

  function open_entry(entry: GanttTask) {
    if (entry.archived) {
      onViewArchivedTask(entry.task);
    } else {
      onViewTask(entry.task);
    }
  }

  function drop_dates(
    task: Task,
    mode: CalendarDragMode,
    grab_day: number,
    target_day: number,
  ): TaskDates | null {
    return calendar_drop_dates(task_dates(task), mode, grab_day, target_day);
  }

  function grab_day_for(range: TaskDateRange, mode: CalendarDragMode): number {
    return mode === "resize-end" ? range.end_day : range.start_day;
  }

  function start_drag(
    event: PointerEvent,
    entry: GanttTask,
    mode: CalendarDragMode,
  ) {
    suppress_click = false;
    if (event.button !== 0 || !entry_editable(entry)) {
      return;
    }
    if (mode !== "move") {
      event.stopPropagation();
    }
    // Without this the press starts a text selection, which the browser
    // auto-scrolls and later drags natively, cancelling the next bar drag.
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
    active_drag = {
      task: entry.task,
      mode,
      grab_day: grab_day_for(entry.range, mode),
      origin_x: event.clientX,
      origin_scroll: scroller?.scrollLeft ?? 0,
      pointer_x: event.clientX,
      moved: false,
    };
    window.addEventListener("pointermove", handle_pointer_move);
    window.addEventListener("pointerup", handle_pointer_up);
    window.addEventListener("pointercancel", stop_drag);
    window.addEventListener("keydown", handle_drag_key);
    scroller?.addEventListener("scroll", update_drag_preview, {
      passive: true,
    });
  }

  function handle_pointer_move(event: PointerEvent) {
    if (active_drag) {
      active_drag.pointer_x = event.clientX;
      update_drag_preview();
    }
  }

  // Measured in timeline pixels, so scrolling during a drag keeps the dragged
  // date under the pointer.
  function update_drag_preview() {
    const drag = active_drag;
    if (!drag) {
      return;
    }
    const distance =
      drag.pointer_x -
      drag.origin_x +
      (scroller?.scrollLeft ?? 0) -
      drag.origin_scroll;
    if (!drag.moved && Math.abs(distance) < DRAG_THRESHOLD) {
      return;
    }
    if (!drag.moved) {
      drag.moved = true;
      edge_scroll_frame = requestAnimationFrame(edge_scroll);
    }
    const offset = Math.round(distance / timeline.day_width);
    if (offset === 0) {
      drag_preview = null;
      return;
    }
    const dates = drop_dates(
      drag.task,
      drag.mode,
      drag.grab_day,
      drag.grab_day + offset,
    );
    drag_preview = {
      task_id: drag.task.id,
      dates,
      range: dates ? task_date_range(dates) : null,
    };
  }

  // Scrolls while the pointer is near or past either visible end of the
  // timeline; the task list covers the viewport's left side.
  function edge_scroll_step(): number {
    const drag = active_drag;
    if (!drag || !scroller) {
      return 0;
    }
    const rect = scroller.getBoundingClientRect();
    const left = rect.left + LIST_WIDTH + EDGE_SCROLL_ZONE;
    const right = rect.right - EDGE_SCROLL_ZONE;
    const step = (depth: number) =>
      Math.ceil(Math.min(1, depth / EDGE_SCROLL_ZONE) * EDGE_SCROLL_MAX_STEP);
    if (drag.pointer_x < left) {
      return -step(left - drag.pointer_x);
    }
    if (drag.pointer_x > right) {
      return step(drag.pointer_x - right);
    }
    return 0;
  }

  function edge_scroll() {
    if (!active_drag) {
      return;
    }
    const step = edge_scroll_step();
    if (step !== 0 && scroller) {
      // The scroll listener updates the preview.
      scroller.scrollLeft += step;
    }
    edge_scroll_frame = requestAnimationFrame(edge_scroll);
  }

  function handle_pointer_up() {
    const drag = active_drag;
    const preview = drag_preview;
    stop_drag();
    if (!drag?.moved) {
      return;
    }
    suppress_click = true;
    if (preview?.dates) {
      onRescheduleTask(drag.task, preview.dates);
    }
  }

  function handle_drag_key(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (active_drag?.moved) {
        suppress_click = true;
      }
      stop_drag();
    }
  }

  function stop_drag() {
    active_drag = null;
    drag_preview = null;
    window.removeEventListener("pointermove", handle_pointer_move);
    window.removeEventListener("pointerup", handle_pointer_up);
    window.removeEventListener("pointercancel", stop_drag);
    window.removeEventListener("keydown", handle_drag_key);
    scroller?.removeEventListener("scroll", update_drag_preview);
    cancelAnimationFrame(edge_scroll_frame);
    edge_scroll_frame = 0;
  }

  // Pressing empty timeline space with a mouse or pen pans the view; touch
  // already scrolls natively, and bars and the task list keep their own
  // presses.
  function start_pan(event: PointerEvent) {
    if (
      event.button !== 0 ||
      event.pointerType === "touch" ||
      active_drag ||
      !scroller ||
      !(event.target instanceof Element) ||
      event.target.closest("button, a, input, [data-gantt-bar]") ||
      event.clientX < scroller.getBoundingClientRect().left + LIST_WIDTH
    ) {
      return;
    }
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
    active_pan = {
      origin_x: event.clientX,
      origin_y: event.clientY,
      scroll_left: scroller.scrollLeft,
      scroll_top: scroller.scrollTop,
    };
    window.addEventListener("pointermove", handle_pan_move);
    window.addEventListener("pointerup", stop_pan);
    window.addEventListener("pointercancel", stop_pan);
  }

  function handle_pan_move(event: PointerEvent) {
    const pan = active_pan;
    if (!pan || !scroller) {
      return;
    }
    const dx = event.clientX - pan.origin_x;
    const dy = event.clientY - pan.origin_y;
    if (!panning && Math.hypot(dx, dy) < DRAG_THRESHOLD) {
      return;
    }
    panning = true;
    scroller.scrollLeft = pan.scroll_left - dx;
    scroller.scrollTop = pan.scroll_top - dy;
  }

  function stop_pan() {
    active_pan = null;
    panning = false;
    window.removeEventListener("pointermove", handle_pan_move);
    window.removeEventListener("pointerup", stop_pan);
    window.removeEventListener("pointercancel", stop_pan);
  }

  function handle_bar_click(entry: GanttTask) {
    if (suppress_click) {
      suppress_click = false;
      return;
    }
    open_entry(entry);
  }

  function handle_bar_key(event: KeyboardEvent, entry: GanttTask) {
    if (
      !entry_editable(entry) ||
      !event.altKey ||
      event.ctrlKey ||
      event.metaKey ||
      (event.key !== "ArrowLeft" && event.key !== "ArrowRight")
    ) {
      return;
    }
    event.preventDefault();
    const mode: CalendarDragMode = event.shiftKey ? "resize-end" : "move";
    if (mode === "resize-end" && entry.range.invalid) {
      return;
    }
    const grab_day = grab_day_for(entry.range, mode);
    const dates = drop_dates(
      entry.task,
      mode,
      grab_day,
      grab_day + (event.key === "ArrowLeft" ? -1 : 1),
    );
    if (dates) {
      onRescheduleTask(entry.task, dates);
    }
  }

  function displayed_range(entry: GanttTask): TaskDateRange {
    return drag_preview?.task_id === entry.task.id &&
      drag_preview.range &&
      !entry.archived
      ? drag_preview.range
      : entry.range;
  }

  function format_task_time(
    date: Date,
    at_midnight: boolean | undefined,
    include_year: boolean,
  ): string {
    return formatDate(date, {
      year: include_year ? "numeric" : undefined,
      month: "short",
      day: "numeric",
      ...(is_date_only(date, at_midnight)
        ? {}
        : { hour: "2-digit", minute: "2-digit", hour12: false }),
    });
  }

  function range_label(dates: TaskDates): string {
    const times = [dates.start_time, dates.due_time].filter(
      (time): time is Date => time !== undefined,
    );
    const include_year = times.some(
      (time) => time.getFullYear() !== today.getFullYear(),
    );
    const start = dates.start_time
      ? format_task_time(
          dates.start_time,
          dates.start_at_midnight,
          include_year,
        )
      : "";
    const due = dates.due_time
      ? format_task_time(dates.due_time, dates.due_at_midnight, include_year)
      : "";
    return start && due
      ? m.calendar_date_range({ start, end: due })
      : start || due;
  }

  function status_label(entry: GanttTask): string {
    return entry.archived
      ? m.explorer_archived()
      : entry.range.kind === "start"
        ? m.calendar_starts()
        : entry.range.invalid
          ? m.calendar_invalid_range()
          : entry.range.kind === "due"
            ? m.task_due_date()
            : "";
  }

  function upper_label(cell: GanttHeaderCell): string {
    const date = date_from_day_number(cell.day);
    return scale === "month"
      ? formatDate(date, { year: "numeric" })
      : formatDate(date, { year: "numeric", month: "long" });
  }

  function lower_label(cell: GanttHeaderCell): string {
    const date = date_from_day_number(cell.day);
    if (scale === "day") {
      return formatNumber(date.getDate());
    }
    return scale === "week"
      ? formatDate(date, { month: "numeric", day: "numeric" })
      : formatDate(date, { month: "short" });
  }

  function cell_title(cell: GanttHeaderCell): string {
    return formatDate(date_from_day_number(cell.day), {
      year: "numeric",
      month: "long",
      day: "numeric",
      weekday: scale === "day" ? "short" : undefined,
    });
  }

  function cell_contains_today(cell: GanttHeaderCell): boolean {
    return (
      today_day >= cell.day &&
      today_day < cell.day + cell.width / timeline.day_width
    );
  }

  function is_weekend(day: number): boolean {
    const weekday = date_from_day_number(day).getDay();
    return weekday === 0 || weekday === 6;
  }

  function group_name(row: Extract<GanttRow, { kind: "group" }>): string {
    return row.column ? row.column.name : m.explorer_archived();
  }
</script>

<section
  class="flex h-full min-h-0 min-w-0 flex-col"
  aria-label={m.gantt_label()}
>
  <header
    class="flex shrink-0 flex-wrap items-center justify-between gap-4 border-b bg-background px-5 py-4"
  >
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <ChartGanttIcon class="size-5 text-muted-foreground" />
        <h2 class="text-xl font-semibold tracking-tight">
          {m.workspace_gantt()}
        </h2>
      </div>
      <p class="mt-1 text-sm text-muted-foreground">
        <span class="font-medium text-foreground"
          >{m.gantt_summary({ count: gantt.task_count })}</span
        >
        {#if gantt.undated_count > 0}
          <span class="mx-1.5" aria-hidden="true">·</span>
          {m.gantt_undated_summary({ count: gantt.undated_count })}
        {/if}
      </p>
    </div>

    <div class="flex items-center gap-2">
      <Button variant="outline" size="sm" onclick={go_to_today}
        >{m.calendar_go_today()}</Button
      >
      <div
        class="flex overflow-hidden rounded-md border bg-background shadow-xs"
        role="group"
        aria-label={m.gantt_scale()}
      >
        {#each [{ id: "day", label: m.gantt_scale_day() }, { id: "week", label: m.gantt_scale_week() }, { id: "month", label: m.gantt_scale_month() }] as option, index (option.id)}
          <Button
            variant={scale === option.id ? "secondary" : "ghost"}
            size="sm"
            class={`rounded-none ${index < 2 ? "border-r" : ""}`}
            aria-pressed={scale === option.id}
            onclick={() => void set_scale(option.id as GanttScale)}
            >{option.label}</Button
          >
        {/each}
      </div>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <Button {...props} variant="outline" size="sm" class="gap-1.5">
              <SlidersHorizontalIcon class="size-3.5" />
              {m.calendar_display()}
              <Badge
                variant="secondary"
                class="ml-0.5 h-5 min-w-5 justify-center rounded-full px-1.5 text-[10px] tabular-nums"
                >{formatNumber(Number(show_archived))}</Badge
              >
            </Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-48">
          <DropdownMenu.CheckboxItem
            bind:checked={show_archived}
            closeOnSelect={false}
          >
            {m.calendar_show_archived()}
          </DropdownMenu.CheckboxItem>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
  </header>

  {#if !read_only}
    <p id={keyboard_hint_id} class="sr-only">{m.gantt_keyboard_hint()}</p>
  {/if}

  {#if gantt.rows.length === 0}
    <Empty.Root class="min-h-0 flex-1">
      <Empty.Header>
        <Empty.Media variant="icon">
          <ChartGanttIcon />
        </Empty.Media>
        <Empty.Title>{m.gantt_empty_title()}</Empty.Title>
        <Empty.Description>{m.gantt_empty_description()}</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <ScrollArea
      bind:viewport={scroller}
      class="min-h-0 flex-1"
      orientation="both"
    >
      <!-- Mouse panning only; the scrollbars, wheel, and keyboard still scroll. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class={`relative ${panning ? "cursor-grabbing select-none **:cursor-grabbing" : ""}`}
        style={`width: ${total_width}px;`}
        onpointerdown={start_pan}
      >
        <div
          class="sticky top-0 z-30 flex border-b bg-background"
          style={`height: ${HEADER_ROW_HEIGHT * 2}px;`}
        >
          <div
            class="sticky left-0 z-10 flex shrink-0 items-end border-r bg-background px-3 pb-1.5 text-xs font-semibold text-muted-foreground"
            style={`width: ${LIST_WIDTH}px;`}
          >
            {m.gantt_task_header()}
          </div>
          <div class="relative shrink-0" style={`width: ${timeline.width}px;`}>
            {#each upper_cells as cell (cell.key)}
              <div
                class="absolute top-0 border-l border-border/70 text-xs font-medium"
                style={`left: ${cell.x}px; width: ${cell.width}px; height: ${HEADER_ROW_HEIGHT}px;`}
              >
                <span
                  class="sticky inline-block truncate px-2 leading-7"
                  style={`left: ${LIST_WIDTH}px; max-width: ${cell.width}px;`}
                  >{upper_label(cell)}</span
                >
              </div>
            {/each}
            {#each lower_cells as cell (cell.key)}
              {@const is_today = cell_contains_today(cell)}
              <div
                class={`absolute flex items-center justify-center overflow-hidden border-l border-t border-border/70 text-[11px] tabular-nums ${
                  is_today
                    ? "font-semibold text-primary"
                    : scale === "day" && is_weekend(cell.day)
                      ? "bg-muted/60 text-muted-foreground"
                      : "text-muted-foreground"
                }`}
                style={`left: ${cell.x}px; width: ${cell.width}px; top: ${HEADER_ROW_HEIGHT}px; height: ${HEADER_ROW_HEIGHT}px;`}
                title={cell_title(cell)}
              >
                <span class="truncate px-1">{lower_label(cell)}</span>
              </div>
            {/each}
          </div>
        </div>

        <div class="relative" style={`height: ${body_height}px;`}>
          <div
            class="pointer-events-none absolute inset-y-0"
            style={`left: ${LIST_WIDTH}px; width: ${timeline.width}px;`}
            aria-hidden="true"
          >
            {#each lower_cells as cell (cell.key)}
              <div
                class={`absolute inset-y-0 border-l border-border/50 ${
                  scale === "day" && is_weekend(cell.day) ? "bg-muted/40" : ""
                }`}
                style={`left: ${cell.x}px; width: ${cell.width}px;`}
              ></div>
            {/each}
            <div
              class="absolute inset-y-0 z-10 w-0.5 -translate-x-1/2 bg-primary/70"
              style={`left: ${(today_day - timeline.first_day + 0.5) * timeline.day_width}px;`}
            ></div>
          </div>

          {#each visible_rows as { row, index } (row.key)}
            {@const row_style = `top: ${index * ROW_HEIGHT}px; height: ${ROW_HEIGHT}px; width: ${total_width}px;`}
            {#if row.kind === "task"}
              {@render task_row(row.entry, row_style)}
            {:else}
              <div class="absolute left-0 flex" style={row_style}>
                <div
                  class="sticky left-0 z-20 flex shrink-0 items-center gap-2 border-r border-b bg-muted px-3 text-xs font-semibold"
                  style={`width: ${LIST_WIDTH}px;`}
                >
                  {#if row.column === null}
                    <ArchiveIcon class="size-3.5 text-muted-foreground" />
                  {/if}
                  <span class="min-w-0 flex-1 truncate">{group_name(row)}</span>
                  <span class="tabular-nums text-muted-foreground"
                    >{formatNumber(row.count)}</span
                  >
                </div>
                <div class="flex-1 border-b bg-muted/40"></div>
              </div>
            {/if}
          {/each}
        </div>
      </div>
    </ScrollArea>
  {/if}
</section>

{#if !read_only}
  <DeleteTaskDialog
    bind:open={delete_confirm_open}
    task_title={delete_target?.title}
    onConfirm={confirm_delete}
  />

  <UnarchiveTaskDialog
    bind:open={unarchive_dialog_open}
    task={unarchive_target}
    {columns}
    onConfirm={onUnarchiveTask}
  />
{/if}

{#snippet task_row(entry: GanttTask, row_style: string)}
  {@const range = displayed_range(entry)}
  {@const editable = entry_editable(entry)}
  {@const previewing = drag_preview?.task_id === entry.task.id}
  {@const invalid_drop = previewing && drag_preview?.dates === null}
  {@const x = (range.start_day - timeline.first_day) * timeline.day_width}
  {@const width = Math.max(
    8,
    (range.end_day - range.start_day + 1) * timeline.day_width,
  )}
  {@const label = range_label(
    previewing && drag_preview?.dates ? drag_preview.dates : entry.task,
  )}
  {@const status = status_label(entry)}
  {@const color = entry_color(entry)}
  <ContextMenu.Root>
    <ContextMenu.Trigger
      class="group/row absolute left-0 flex"
      style={row_style}
    >
      <div
        class="sticky left-0 z-20 flex shrink-0 items-center border-r border-b bg-background group-hover/row:bg-muted"
        style={`width: ${LIST_WIDTH}px;`}
      >
        <button
          type="button"
          tabindex="-1"
          class="flex h-full min-w-0 flex-1 items-center gap-2 px-3 text-left text-sm"
          title={[entry.task.title, entry.column?.name, label, status]
            .filter(Boolean)
            .join(" · ")}
          onclick={() => open_entry(entry)}
        >
          <span
            class="h-4 w-1 shrink-0 rounded-full"
            style={`background-color: ${color}`}
          ></span>
          <span
            class={`min-w-0 flex-1 truncate ${entry.archived ? "text-muted-foreground line-through" : "font-medium"}`}
            >{entry.task.title}</span
          >
        </button>
      </div>
      <div
        class="relative shrink-0 border-b border-border/50 group-hover/row:bg-muted/30"
        style={`width: ${timeline.width}px;`}
      >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          data-gantt-bar
          class="group/bar absolute top-1/2 flex h-6 -translate-y-1/2 items-center"
          style={`left: ${x}px; width: ${width}px;`}
          ondblclick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            class={`flex h-full min-w-0 flex-1 items-center gap-1 overflow-hidden rounded-md border px-1.5 text-left text-xs transition-[box-shadow,border-color,opacity] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
              entry.archived
                ? "border-dotted opacity-60"
                : range.invalid
                  ? "border-destructive/60"
                  : ""
            } ${invalid_drop ? "border-dashed border-destructive ring-2 ring-destructive/40" : ""} ${
              previewing && !invalid_drop
                ? "border-dashed border-primary shadow-md"
                : "shadow-xs hover:shadow-sm"
            } ${editable ? "cursor-grab touch-none select-none active:cursor-grabbing" : "cursor-pointer"}`}
            style={`background-color: color-mix(in oklab, ${color} 22%, var(--card));${
              previewing
                ? ""
                : ` border-color: color-mix(in oklab, ${color} 65%, var(--border));`
            }`}
            title={[entry.task.title, entry.column?.name, label, status]
              .filter(Boolean)
              .join(" · ")}
            aria-label={m.calendar_span_label({
              title: entry.task.title,
              range: [label, status].filter(Boolean).join(" · "),
            })}
            aria-describedby={editable ? keyboard_hint_id : undefined}
            aria-keyshortcuts={editable
              ? "Alt+ArrowLeft Alt+ArrowRight Alt+Shift+ArrowLeft Alt+Shift+ArrowRight"
              : undefined}
            onpointerdown={(event) => start_drag(event, entry, "move")}
            onclick={() => handle_bar_click(entry)}
            onkeydown={(event) => handle_bar_key(event, entry)}
          >
            {#if width < 28}
              <!-- Too narrow for an icon; the status stays in the labels. -->
            {:else if entry.archived}
              <ArchiveIcon class="size-3 shrink-0 text-muted-foreground" />
            {:else if range.kind === "start"}
              <PlayIcon class="size-3 shrink-0 text-muted-foreground" />
            {:else if range.invalid}
              <TriangleAlertIcon class="size-3 shrink-0 text-destructive" />
            {/if}
            {#if width >= 56}
              <span class="min-w-0 flex-1 truncate font-medium"
                >{entry.task.title}</span
              >
            {/if}
          </button>
          {#if editable && !entry.range.invalid}
            {@render resize_handle(entry, "resize-start")}
            {@render resize_handle(entry, "resize-end")}
          {/if}
          <span
            class={`pointer-events-none absolute left-full ml-2 whitespace-nowrap text-[11px] tabular-nums ${previewing ? "font-medium text-primary" : "text-muted-foreground"}`}
            aria-hidden="true"
          >
            {width < 56 ? `${entry.task.title} · ${label}` : label}
          </span>
        </div>
      </div>
    </ContextMenu.Trigger>
    <ContextMenu.Content>
      {#if entry.archived}
        <ContextMenu.Item onclick={() => open_entry(entry)}>
          <FullscreenIcon />
          {m.task_view_details()}
        </ContextMenu.Item>
        {#if !read_only}
          <ContextMenu.Item onclick={() => ask_to_unarchive(entry.task)}>
            <UndoIcon />
            {m.calendar_unarchive()}
          </ContextMenu.Item>
        {/if}
      {:else}
        <TaskActionItems
          context
          onView={() => open_entry(entry)}
          onEdit={editable ? () => onEditTask(entry.task) : undefined}
          onArchive={editable ? () => onArchiveTask(entry.task) : undefined}
          onDelete={editable ? () => ask_to_delete(entry.task) : undefined}
        >
          {#snippet extra()}
            {#if editable}
              <ContextMenu.Item onclick={() => onDuplicateTask(entry.task)}>
                <CopyIcon />
                {m.task_duplicate_action()}
              </ContextMenu.Item>
              <ContextMenu.Item onclick={() => onSaveAsTemplate(entry.task)}>
                <LayoutTemplateIcon />
                {m.task_template_save()}
              </ContextMenu.Item>
              <ContextMenu.Item onclick={() => onExportTask(entry.task)}>
                <Share2Icon />
                {m.task_share()}
              </ContextMenu.Item>
            {/if}
          {/snippet}
        </TaskActionItems>
      {/if}
    </ContextMenu.Content>
  </ContextMenu.Root>
{/snippet}

{#snippet resize_handle(entry: GanttTask, mode: "resize-start" | "resize-end")}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class={`absolute inset-y-0 z-10 flex w-2 cursor-ew-resize touch-none select-none items-center justify-center opacity-0 transition-opacity group-hover/bar:opacity-100 ${
      mode === "resize-start" ? "-left-1" : "-right-1"
    }`}
    title={mode === "resize-start"
      ? m.calendar_resize_start()
      : m.calendar_resize_end()}
    aria-hidden="true"
    onpointerdown={(event) => start_drag(event, entry, mode)}
  >
    <span class="h-3.5 w-1 rounded-full bg-foreground/50 hover:bg-foreground/80"
    ></span>
  </div>
{/snippet}
