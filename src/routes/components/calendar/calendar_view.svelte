<script lang="ts">
  import { formatDate, formatNumber } from "$lib/i18n";
  import { getLocale } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import TriangleAlertIcon from "@lucide/svelte/icons/triangle-alert";
  import ChevronLeftIcon from "@lucide/svelte/icons/chevron-left";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import ClockIcon from "@lucide/svelte/icons/clock";
  import PlayIcon from "@lucide/svelte/icons/play";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import LayoutTemplateIcon from "@lucide/svelte/icons/layout-template";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import TrashIcon from "@lucide/svelte/icons/trash-2";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import UndoIcon from "@lucide/svelte/icons/undo-2";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import SlidersHorizontalIcon from "@lucide/svelte/icons/sliders-horizontal";
  import Share2Icon from "@lucide/svelte/icons/share-2";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import { onMount } from "svelte";
  import { DragDropProvider } from "@dnd-kit-svelte/svelte";

  import type { Column } from "../../type/column.svelte";
  import type { Archive } from "../../type/archive.svelte";
  import type { Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";
  import DeleteTaskDialog from "../task/delete_task_dialog.svelte";
  import UnarchiveTaskDialog from "../archive/unarchive_task_dialog.svelte";
  import CalendarDayDropZone from "./calendar_day_drop_zone.svelte";
  import CalendarTaskDraggable from "./calendar_task_draggable.svelte";
  import CalendarExportDialog from "../dialog/calendar_export_dialog.svelte";
  import {
    build_calendar,
    calendar_drop_dates,
    calendar_task_key,
    count_due_tasks,
    date_key,
    start_of_day,
    type CalendarDay,
    type CalendarDragData,
    type CalendarDragMode,
    type CalendarSpanSegment,
    type CalendarTask,
    type CalendarViewMode,
    type CalendarWeek,
  } from "./calendar";
  import {
    day_number,
    is_date_only,
    ranges_intersect,
    task_date_range,
    task_dates,
    type TaskDateRangeKind,
    type TaskDates,
  } from "../../utils/task-date-range";

  // Lanes for multi-day tasks above each week's single-day lists.
  const MONTH_SPAN_LANES = 3;
  const WEEK_SPAN_LANES = 8;

  const initial_today = start_of_day(new Date());

  interface CalendarViewProps {
    columns: Column[];
    archives?: Archive[];
    search_text?: string;
    search_task_ids?: ReadonlySet<string> | null;
    visible_date?: Date;
    view_mode?: CalendarViewMode;
    show_archived?: boolean;
    show_recurring_previews?: boolean;
    onViewTask?: (task: Task) => void;
    onViewArchivedTask?: (task: Task) => void;
    onEditTask?: (task: Task) => void;
    onDuplicateTask?: (task: Task) => void;
    onSaveAsTemplate?: (task: Task) => void;
    onExportTask?: (task: Task) => void;
    onAddTask?: (date: Date) => void;
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
    visible_date = $bindable(new Date(initial_today)),
    view_mode = $bindable("month"),
    show_archived = $bindable(false),
    show_recurring_previews = $bindable(true),
    onViewTask = () => {},
    onViewArchivedTask = () => {},
    onEditTask = () => {},
    onDuplicateTask = () => {},
    onSaveAsTemplate = () => {},
    onExportTask = () => {},
    onAddTask = () => {},
    onArchiveTask = () => {},
    onUnarchiveTask = () => {},
    onDeleteTask = () => {},
    onRescheduleTask = () => {},
    read_only = false,
  }: CalendarViewProps = $props();

  const weekdays = $derived(
    Array.from({ length: 7 }, (_, day) =>
      new Date(2024, 0, 7 + day).toLocaleDateString(getLocale(), {
        weekday: "short",
      }),
    ),
  );
  let today = $state(initial_today);
  let delete_confirm_open = $state(false);
  let delete_target = $state<Task | null>(null);
  let unarchive_dialog_open = $state(false);
  let unarchive_target = $state<Task | null>(null);
  let archive_focus_restore_target: string | null = null;
  let export_dialog_open = $state(false);

  interface ActiveCalendarDrag {
    task: Task;
    mode: CalendarDragMode;
    grab_day: number;
  }

  interface CalendarDropPreview {
    start_day: number;
    end_day: number;
    valid: boolean;
    task: Task;
    dates: TaskDates | null;
    /** Kind of the resulting range; a span draws as a bar, others as a card. */
    kind: TaskDateRangeKind | null;
  }

  interface CalendarGhostSegment {
    column: number;
    length: number;
    lane: number;
    continues_before: boolean;
    continues_after: boolean;
  }

  // Drag state stays outside the layout derivation so dragging never
  // recomputes lanes.
  let active_drag: ActiveCalendarDrag | null = null;
  let span_grab: { id: string; day: number } | null = null;
  let drop_preview = $state<CalendarDropPreview | null>(null);

  function format_time(date: Date): string {
    return date.toLocaleTimeString(getLocale(), {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  }

  function has_time(date: Date, at_midnight: boolean | undefined): boolean {
    return !is_date_only(date, at_midnight);
  }

  function change_period(offset: number) {
    if (view_mode === "week") {
      const next = new Date(visible_date);
      next.setDate(next.getDate() + offset * 7);
      visible_date = next;
    } else {
      visible_date = new Date(
        visible_date.getFullYear(),
        visible_date.getMonth() + offset,
        1,
      );
    }
  }

  function set_view_mode(mode: CalendarViewMode) {
    if (mode === view_mode) {
      return;
    }
    if (
      mode === "week" &&
      visible_date.getFullYear() === today.getFullYear() &&
      visible_date.getMonth() === today.getMonth()
    ) {
      visible_date = new Date(today);
    }
    view_mode = mode;
  }

  function go_to_today() {
    const current_today = start_of_day(new Date());
    today = current_today;
    visible_date = new Date(current_today);
  }

  function refresh_today() {
    const current_today = start_of_day(new Date());
    if (date_key(current_today) !== date_key(today)) {
      today = current_today;
    }
  }

  onMount(() => {
    let midnight_timeout: number | undefined;

    function schedule_midnight_refresh() {
      const now = new Date();
      const next_day = new Date(
        now.getFullYear(),
        now.getMonth(),
        now.getDate() + 1,
      );
      midnight_timeout = window.setTimeout(
        () => {
          refresh_today();
          schedule_midnight_refresh();
        },
        next_day.getTime() - now.getTime() + 1000,
      );
    }

    function handle_visibility_change() {
      if (!document.hidden) {
        refresh_today();
      }
    }

    window.addEventListener("focus", refresh_today);
    document.addEventListener("visibilitychange", handle_visibility_change);
    schedule_midnight_refresh();

    return () => {
      window.removeEventListener("focus", refresh_today);
      document.removeEventListener(
        "visibilitychange",
        handle_visibility_change,
      );
      if (midnight_timeout !== undefined) {
        window.clearTimeout(midnight_timeout);
      }
    };
  });

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

  function drag_data(
    entry: CalendarTask,
    mode: CalendarDragMode = "move",
  ): CalendarDragData {
    return { task_id: entry.task.id, mode, grab_day: entry.range.start_day };
  }

  function segment_drag_id(segment: CalendarSpanSegment): string {
    return `calendar-task:${segment.entry.task.id}:${segment.key}`;
  }

  // Captured before the drag starts so a move keeps the grabbed day under the
  // pointer, wherever the bar was grabbed.
  function record_span_grab(
    event: PointerEvent,
    segment: CalendarSpanSegment,
    week: CalendarWeek,
  ) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const offset =
      rect.width > 0
        ? Math.floor(
            ((event.clientX - rect.left) / rect.width) * segment.length,
          )
        : 0;
    span_grab = {
      id: segment_drag_id(segment),
      day:
        day_number(week.days[segment.column].date) +
        Math.min(segment.length - 1, Math.max(0, offset)),
    };
  }

  function drop_target_day(event: any): number | null {
    const target_id = event.operation?.target?.id?.toString();
    if (!target_id?.startsWith("calendar-day:")) {
      return null;
    }
    const target_key = target_id.slice("calendar-day:".length);
    const target = calendar_days.find((day) => day.key === target_key);
    return target ? day_number(target.date) : null;
  }

  function handle_drag_start(event: any) {
    const source = event.operation?.source;
    active_drag = null;
    drop_preview = null;
    if (source?.type !== "calendar-task") {
      return;
    }
    const data = source.data as CalendarDragData | undefined;
    const task = data
      ? columns
          .flatMap((column) => column.tasks)
          .find((item) => item.id === data.task_id)
      : undefined;
    if (!data || !task) {
      return;
    }
    const grab = span_grab;
    const grab_day =
      data.mode === "move" && grab && grab.id === source.id?.toString()
        ? grab.day
        : data.grab_day;
    active_drag = { task, mode: data.mode, grab_day };
  }

  function handle_drag_over(event: any) {
    const drag = active_drag;
    const target_day = drop_target_day(event);
    if (!drag || target_day === null) {
      drop_preview = null;
      return;
    }
    const dates = calendar_drop_dates(
      task_dates(drag.task),
      drag.mode,
      drag.grab_day,
      target_day,
    );
    const range = dates ? task_date_range(dates) : null;
    if (range) {
      drop_preview = {
        ...range,
        valid: true,
        task: drag.task,
        dates,
      };
      return;
    }
    const current = task_date_range(drag.task);
    const unchanged =
      drag.mode === "move"
        ? target_day === drag.grab_day
        : target_day ===
          (drag.mode === "resize-start"
            ? current?.start_day
            : current?.end_day);
    drop_preview = unchanged
      ? null
      : {
          start_day: target_day,
          end_day: target_day,
          valid: false,
          task: drag.task,
          dates: null,
          kind: null,
        };
  }

  function handle_drag_end(event: any) {
    const drag = active_drag;
    active_drag = null;
    span_grab = null;
    drop_preview = null;
    const target_day = drop_target_day(event);
    if (!drag || event.canceled || target_day === null) {
      return;
    }
    const dates = calendar_drop_dates(
      task_dates(drag.task),
      drag.mode,
      drag.grab_day,
      target_day,
    );
    if (dates) {
      onRescheduleTask(drag.task, dates);
    }
  }

  function day_preview(day: CalendarDay): "valid" | "invalid" | null {
    // Valid drops are drawn as a ghost bar or card instead.
    if (!drop_preview || drop_preview.valid) {
      return null;
    }
    const day_index = day_number(day.date);
    return day_index >= drop_preview.start_day &&
      day_index <= drop_preview.end_day
      ? drop_preview.valid
        ? "valid"
        : "invalid"
      : null;
  }

  // The dragged range clipped to one week, on the lane the task already uses.
  function ghost_segments(
    preview: CalendarDropPreview | null,
    week: CalendarWeek,
  ): CalendarGhostSegment[] {
    if (!preview?.valid || preview.kind !== "span") {
      return [];
    }
    const first_day = day_number(week.days[0].date);
    const last_day = first_day + week.days.length - 1;
    if (!ranges_intersect(preview, first_day, last_day)) {
      return [];
    }
    const start = Math.max(preview.start_day, first_day);
    const end = Math.min(preview.end_day, last_day);
    const column = start - first_day;
    const length = end - start + 1;
    // Prefer the task's own lane, otherwise the first lane no other bar uses;
    // past the last lane the week grows a row while dragging.
    const own_lane = week.segments.find(
      (segment) => segment.entry.task.id === preview.task.id,
    )?.lane;
    const lane_free = (lane: number) =>
      !week.segments.some(
        (segment) =>
          segment.lane === lane &&
          segment.entry.task.id !== preview.task.id &&
          segment.column < column + length &&
          segment.column + segment.length > column,
      );
    let lane = own_lane !== undefined && lane_free(own_lane) ? own_lane : 0;
    while (!lane_free(lane)) {
      lane += 1;
    }
    return [
      {
        column,
        length,
        lane,
        continues_before: preview.start_day < first_day,
        continues_after: preview.end_day > last_day,
      },
    ];
  }

  function day_ghost(day: CalendarDay): CalendarDropPreview | null {
    return drop_preview?.valid &&
      drop_preview.kind !== "span" &&
      drop_preview.start_day === day_number(day.date)
      ? drop_preview
      : null;
  }

  function task_time_label(
    dates: TaskDates,
    kind: TaskDateRangeKind | null,
  ): string {
    const { start_time, due_time } = dates;
    if (
      kind === "single" &&
      start_time &&
      has_time(start_time, dates.start_at_midnight) &&
      due_time &&
      has_time(due_time, dates.due_at_midnight)
    ) {
      return m.calendar_date_range({
        start: format_time(start_time),
        end: format_time(due_time),
      });
    }
    const [anchor_time, at_midnight] =
      kind === "start"
        ? [start_time, dates.start_at_midnight]
        : [due_time, dates.due_at_midnight];
    return anchor_time && has_time(anchor_time, at_midnight)
      ? format_time(anchor_time)
      : "";
  }

  function is_drag_source(entry: CalendarTask): boolean {
    return (
      !!drop_preview?.valid &&
      !entry.preview &&
      !entry.archived &&
      drop_preview.task.id === entry.task.id
    );
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

  function task_range_label(task: TaskDates): string {
    if (!task.start_time || !task.due_time) {
      return "";
    }
    const include_year =
      task.start_time.getFullYear() !== task.due_time.getFullYear() ||
      task.due_time.getFullYear() !== today.getFullYear();
    return m.calendar_date_range({
      start: format_task_time(
        task.start_time,
        task.start_at_midnight,
        include_year,
      ),
      end: format_task_time(task.due_time, task.due_at_midnight, include_year),
    });
  }

  function entry_open(entry: CalendarTask) {
    if (entry.archived) {
      onViewArchivedTask(entry.task);
    } else {
      onViewTask(entry.task);
    }
  }

  function entry_color(entry: CalendarTask): string {
    return entry.archived || entry.preview
      ? "var(--muted-foreground)"
      : display_task_color(entry.task.color) || "var(--muted-foreground)";
  }

  function entry_editable(entry: CalendarTask): boolean {
    return !read_only && !entry.archived && !entry.preview;
  }

  // Shared by single-day cards and multi-day bars so both read as one item.
  function task_card_class(entry: CalendarTask): string {
    const state = entry.archived
      ? "border border-dotted border-muted-foreground/40 bg-muted/30 opacity-55 hover:opacity-70"
      : entry.preview
        ? "border border-dashed border-muted-foreground/45 bg-transparent opacity-70 hover:opacity-85"
        : "border border-foreground/30 bg-card shadow-xs hover:border-foreground/50 hover:shadow-sm";
    const cursor = !entry_editable(entry)
      ? "cursor-pointer"
      : "cursor-grab active:cursor-grabbing select-none touch-none";
    return `transition-[border-color,background-color,box-shadow,opacity] ${state} ${cursor}`;
  }

  function task_title_class(entry: CalendarTask): string {
    if (entry.archived) {
      return "text-muted-foreground line-through";
    }
    if (entry.preview) {
      return "text-muted-foreground";
    }
    return "font-medium";
  }

  function format_period_label(date: Date, mode: CalendarViewMode): string {
    if (mode === "month") {
      return date.toLocaleDateString(getLocale(), {
        year: "numeric",
        month: "long",
      });
    }
    const start = start_of_day(date);
    start.setDate(start.getDate() - start.getDay());
    const end = new Date(start);
    end.setDate(end.getDate() + 6);
    if (start.getFullYear() !== end.getFullYear()) {
      return `${start.toLocaleDateString(getLocale(), { month: "short", day: "numeric", year: "numeric" })} - ${end.toLocaleDateString(getLocale(), { month: "short", day: "numeric", year: "numeric" })}`;
    }
    if (start.getMonth() !== end.getMonth()) {
      return `${start.toLocaleDateString(getLocale(), { month: "long", day: "numeric" })} - ${end.toLocaleDateString(getLocale(), { month: "long", day: "numeric", year: "numeric" })}`;
    }
    return new Intl.DateTimeFormat(getLocale(), {
      year: "numeric",
      month: "long",
      day: "numeric",
    }).formatRange(start, end);
  }

  let visible_period_label = $derived(
    format_period_label(visible_date, view_mode),
  );

  let due_task_count = $derived(
    count_due_tasks(
      columns,
      search_text,
      archives,
      show_archived,
      search_task_ids,
    ),
  );

  let calendar = $derived(
    build_calendar(
      columns,
      visible_date,
      today,
      search_text,
      archives,
      show_archived,
      show_recurring_previews,
      view_mode,
      search_task_ids,
      view_mode === "month" ? MONTH_SPAN_LANES : WEEK_SPAN_LANES,
    ),
  );
  let calendar_days = $derived(calendar.days);
  let today_day = $derived(day_number(today));

  let current_period_task_count = $derived(
    calendar_days.reduce(
      (count, day) => count + (day.in_current_month ? day.tasks.length : 0),
      0,
    ),
  );
  let current_period_preview_count = $derived(
    calendar_days.reduce(
      (count, day) =>
        count +
        (day.in_current_month
          ? day.tasks.filter((entry) => entry.preview).length
          : 0),
      0,
    ),
  );
  let enabled_display_option_count = $derived(
    Number(show_recurring_previews) + Number(show_archived),
  );
</script>

<section
  class="flex min-w-[820px] flex-col gap-4 p-5"
  aria-label={m.calendar_label()}
>
  <header
    class="sticky top-0 z-20 -mx-5 -mt-5 flex items-center justify-between gap-4 border-b bg-background/95 px-5 py-4 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85"
  >
    <div>
      <div class="flex items-center gap-2">
        <CalendarDaysIcon class="size-5 text-muted-foreground" />
        <h2 class="text-xl font-semibold tracking-tight">
          {visible_period_label}
        </h2>
      </div>
      <p class="mt-1 text-sm text-muted-foreground">
        {m.calendar_due_summary({ count: due_task_count })}
        <span class="mx-1.5" aria-hidden="true">·</span>
        <span class="font-medium text-foreground">
          {view_mode === "week"
            ? m.calendar_week_summary({ count: current_period_task_count })
            : m.calendar_month_summary({ count: current_period_task_count })}
          {#if current_period_preview_count > 0}
            <span class="font-normal text-muted-foreground">
              ({m.calendar_recurring_summary({
                count: current_period_preview_count,
              })})
            </span>
          {/if}
        </span>
      </p>
      <div
        class="mt-2 flex flex-wrap items-center gap-3 text-[11px] text-muted-foreground"
        aria-label={m.calendar_legend()}
      >
        <span class="inline-flex items-center gap-1.5">
          <span
            class="size-2.5 rounded-sm border-2 border-foreground/60 bg-card shadow-xs"
          ></span>
          {m.explorer_active()}
        </span>
        <span class="inline-flex items-center gap-1.5">
          <span
            class="h-2.5 w-5 rounded-sm border-2 border-foreground/60 bg-card shadow-xs"
          ></span>
          {m.calendar_multi_day()}
        </span>
        <span class="inline-flex items-center gap-1.5 opacity-70">
          <span
            class="size-2.5 rounded-sm border border-dashed border-muted-foreground/60 bg-transparent"
          ></span>
          {m.calendar_recurring_preview()}
        </span>
        <span class="inline-flex items-center gap-1.5 opacity-55">
          <span
            class="size-2.5 rounded-sm border border-dotted border-muted-foreground/50 bg-muted/30"
          ></span>
          {m.explorer_archived()}
        </span>
      </div>
    </div>

    <div class="flex items-center gap-2">
      {#if !read_only}
        <Button
          variant="outline"
          size="sm"
          class="gap-1.5"
          onclick={() => {
            export_dialog_open = true;
          }}
        >
          <DownloadIcon class="size-3.5" />
          {m.calendar_export()}
        </Button>
      {/if}
      <Button variant="outline" size="sm" onclick={go_to_today}
        >{m.calendar_go_today()}</Button
      >
      <div
        class="flex overflow-hidden rounded-md border bg-background shadow-xs"
      >
        <Button
          variant="ghost"
          size="icon-sm"
          class="rounded-none border-r"
          aria-label={view_mode === "week"
            ? m.calendar_previous_week()
            : m.calendar_previous_month()}
          title={view_mode === "week"
            ? m.calendar_previous_week()
            : m.calendar_previous_month()}
          onclick={() => change_period(-1)}
        >
          <ChevronLeftIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          class="rounded-none"
          aria-label={view_mode === "week"
            ? m.calendar_next_week()
            : m.calendar_next_month()}
          title={view_mode === "week"
            ? m.calendar_next_week()
            : m.calendar_next_month()}
          onclick={() => change_period(1)}
        >
          <ChevronRightIcon />
        </Button>
      </div>
      <div
        class="flex overflow-hidden rounded-md border bg-background shadow-xs"
        aria-label={m.calendar_view()}
      >
        <Button
          variant={view_mode === "month" ? "secondary" : "ghost"}
          size="sm"
          class="rounded-none border-r"
          aria-pressed={view_mode === "month"}
          onclick={() => set_view_mode("month")}>{m.calendar_month()}</Button
        >
        <Button
          variant={view_mode === "week" ? "secondary" : "ghost"}
          size="sm"
          class="rounded-none"
          aria-pressed={view_mode === "week"}
          onclick={() => set_view_mode("week")}>{m.calendar_week()}</Button
        >
      </div>
      {#if !read_only}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger>
            {#snippet child({ props })}
              <Button {...props} variant="outline" size="sm" class="gap-1.5">
                <SlidersHorizontalIcon class="size-3.5" />
                {m.calendar_display()}
                <Badge
                  variant="secondary"
                  class="ml-0.5 h-5 min-w-5 justify-center rounded-full px-1.5 text-[10px] tabular-nums"
                  >{formatNumber(enabled_display_option_count)}</Badge
                >
              </Button>
            {/snippet}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="end" class="min-w-48">
            <DropdownMenu.Label>{m.calendar_items()}</DropdownMenu.Label>
            <DropdownMenu.Separator />
            <DropdownMenu.CheckboxItem
              bind:checked={show_recurring_previews}
              closeOnSelect={false}
            >
              {m.calendar_show_recurring()}
            </DropdownMenu.CheckboxItem>
            <DropdownMenu.CheckboxItem
              bind:checked={show_archived}
              closeOnSelect={false}
            >
              {m.calendar_show_archived()}
            </DropdownMenu.CheckboxItem>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      {/if}
    </div>
  </header>

  <DragDropProvider
    onDragStart={handle_drag_start}
    onDragOver={handle_drag_over}
    onDragEnd={handle_drag_end}
  >
    <div
      class="flex flex-none flex-col gap-px overflow-hidden rounded-xl border bg-border shadow-sm"
      style={`min-height: max(calc(100vh - 12rem), ${view_mode === "month" ? 36 : 18}rem);`}
    >
      <div class="grid grid-cols-7 gap-px">
        {#each weekdays as weekday}
          <div
            class="bg-muted/80 px-3 py-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground"
          >
            {weekday}
          </div>
        {/each}
      </div>

      {#each calendar.weeks as week (week.key)}
        {@const has_hidden = week.hidden.some((entries) => entries.length > 0)}
        {@const ghosts = ghost_segments(drop_preview, week)}
        {@const lane_count = Math.max(
          week.lane_count,
          ...ghosts.map((ghost) => ghost.lane + 1),
        )}
        <div
          class="grid flex-1 grid-cols-7 gap-x-px"
          style={`grid-template-rows: auto${
            lane_count > 0
              ? ` repeat(${lane_count}, minmax(var(--calendar-lane-height), auto))`
              : ""
          }${has_hidden ? " auto" : ""} minmax(1.75rem, 1fr); --calendar-lane-height: 1.75rem;`}
        >
          {#each week.days as day, day_index (day.key)}
            {@const singles = week.singles[day_index]}
            {@const hidden = week.hidden[day_index]}
            {@const day_count =
              singles.length +
              hidden.length +
              week.segments.filter(
                (segment) =>
                  segment.column <= day_index &&
                  segment.column + segment.length > day_index,
              ).length}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <CalendarDayDropZone
              day_key={day.key}
              disabled={read_only}
              preview={day_preview(day)}
              class={`group/day row-span-full grid min-h-0 grid-rows-subgrid overflow-hidden bg-background px-1.5 pt-1.5 pb-1 ${
                day.in_current_month
                  ? ""
                  : "bg-muted/30 text-muted-foreground/50"
              } ${day.is_today ? "ring-2 ring-inset ring-primary/35" : ""}`}
              style={`grid-column: ${day_index + 1};`}
              title={read_only ? undefined : m.calendar_add_hint()}
              ondblclick={read_only
                ? undefined
                : () => onAddTask(new Date(day.date))}
            >
              <div class="mb-1 flex items-center justify-between">
                <span
                  class={`flex size-6 items-center justify-center rounded-full text-xs tabular-nums ${
                    day.is_today
                      ? "bg-primary font-semibold text-primary-foreground"
                      : ""
                  }`}
                >
                  {day.date.getDate()}
                </span>
                <div class="flex items-center gap-1">
                  {#if day_count > 0}
                    <span class="text-[11px] tabular-nums text-muted-foreground"
                      >{formatNumber(day_count)}</span
                    >
                  {/if}
                  {#if !read_only}
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      class="size-6 opacity-0 transition-opacity group-hover/day:opacity-100 focus-visible:opacity-100"
                      aria-label={m.calendar_add_due({
                        date: day.date.toLocaleDateString(getLocale(), {
                          year: "numeric",
                          month: "long",
                          day: "numeric",
                        }),
                      })}
                      title={m.calendar_add()}
                      onclick={(event) => {
                        event.stopPropagation();
                        onAddTask(new Date(day.date));
                      }}
                      ondblclick={(event) => event.stopPropagation()}
                    >
                      <PlusIcon />
                    </Button>
                  {/if}
                </div>
              </div>

              {#if has_hidden}
                <div
                  class="flex min-h-5 items-center"
                  style={`grid-row: ${lane_count + 2};`}
                >
                  {#if hidden.length > 0}
                    {@render hidden_tasks(day, hidden)}
                  {/if}
                </div>
              {/if}

              <div
                class="min-h-0 overflow-x-hidden overflow-y-auto pt-0.5 [scrollbar-width:thin]"
                style="grid-row: -2;"
              >
                <div class="flex flex-col gap-1">
                  {#if day_ghost(day)}
                    {@render ghost_card(drop_preview!)}
                  {/if}
                  {#each singles as entry (calendar_task_key(entry))}
                    {@render single_task(entry)}
                  {/each}
                </div>
              </div>
            </CalendarDayDropZone>
          {/each}

          {#each week.segments as segment (segment.key)}
            {@render span_task(segment, week)}
          {/each}

          {#each ghosts as ghost (ghost.column)}
            {@render ghost_bar(ghost)}
          {/each}
        </div>
      {/each}
    </div>
  </DragDropProvider>
</section>

{#if !read_only}
  <CalendarExportDialog
    bind:open={export_dialog_open}
    days={calendar_days}
    {columns}
    {archives}
    {search_text}
    {visible_date}
    {today}
    {show_archived}
    {show_recurring_previews}
    {view_mode}
    period_label={visible_period_label}
  />

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

{#snippet task_menu(entry: CalendarTask)}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon-sm"
          class="size-5 bg-card/90 text-muted-foreground shadow-sm hover:text-foreground"
          aria-label={m.explorer_actions_named({
            name: entry.task.title,
          })}
          title={m.task_menu()}
        >
          <EllipsisIcon class="size-3.5" />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content
      align="end"
      class="min-w-36"
      onCloseAutoFocus={(event) => {
        if (archive_focus_restore_target === entry.task.id) {
          event.preventDefault();
        }
        archive_focus_restore_target = null;
      }}
    >
      <DropdownMenu.Item onclick={() => entry_open(entry)}>
        <FullscreenIcon />
        {m.task_view_details()}
      </DropdownMenu.Item>
      {#if entry.archived}
        <DropdownMenu.Item onclick={() => ask_to_unarchive(entry.task)}>
          <UndoIcon />
          {m.calendar_unarchive()}
        </DropdownMenu.Item>
      {:else if !entry.preview}
        <DropdownMenu.Item onclick={() => onEditTask(entry.task)}>
          <PencilIcon />
          {m.task_edit_action()}
        </DropdownMenu.Item>
        <DropdownMenu.Item onclick={() => onDuplicateTask(entry.task)}>
          <CopyIcon />
          {m.task_duplicate_action()}
        </DropdownMenu.Item>
        <DropdownMenu.Item onclick={() => onSaveAsTemplate(entry.task)}>
          <LayoutTemplateIcon />
          {m.task_template_save()}
        </DropdownMenu.Item>
        <DropdownMenu.Item onclick={() => onExportTask(entry.task)}>
          <Share2Icon />
          {m.task_share()}
        </DropdownMenu.Item>
        <DropdownMenu.Item
          onSelect={() => {
            archive_focus_restore_target = entry.task.id;
            onArchiveTask(entry.task);
          }}
        >
          <ArchiveIcon />
          {m.task_archive_action()}
        </DropdownMenu.Item>
        <DropdownMenu.Separator />
        <DropdownMenu.Item
          variant="destructive"
          onclick={() => ask_to_delete(entry.task)}
        >
          <TrashIcon />
          {m.task_delete()}
        </DropdownMenu.Item>
      {/if}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
{/snippet}

{#snippet task_status_icon(entry: CalendarTask)}
  {#if entry.archived}
    <ArchiveIcon class="size-3 shrink-0" aria-label={m.explorer_archived()} />
  {:else if entry.preview}
    <Repeat2Icon class="size-3 shrink-0" aria-label={m.common_preview()} />
  {:else if entry.range.kind === "start"}
    <PlayIcon class="size-3 shrink-0" aria-label={m.calendar_starts()} />
  {:else if entry.range.invalid}
    <TriangleAlertIcon
      class="size-3 shrink-0 text-destructive"
      aria-label={m.calendar_invalid_range()}
    />
  {/if}
{/snippet}

{#snippet single_task(entry: CalendarTask)}
  {@const editable = entry_editable(entry)}
  {@const resizable = editable && !entry.range.invalid}
  {@const time_label = task_time_label(entry.task, entry.range.kind)}
  {@const status_label = entry.archived
    ? m.explorer_archived()
    : entry.preview
      ? m.common_preview()
      : entry.range.kind === "start"
        ? m.calendar_starts()
        : entry.range.invalid
          ? m.calendar_invalid_range()
          : ""}
  <div
    class={`group relative min-w-0 transition-opacity ${is_drag_source(entry) ? "opacity-30" : ""}`}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <CalendarTaskDraggable
      task_id={entry.task.id}
      instance_key={calendar_task_key(entry)}
      data={drag_data(entry)}
      disabled={!editable}
      class={`overflow-hidden rounded-md text-xs ${task_card_class(entry)}`}
      oncontextmenu={read_only
        ? undefined
        : (event) => {
            event.preventDefault();
            if (editable) {
              onEditTask(entry.task);
            }
          }}
    >
      <button
        type="button"
        class="flex h-6 w-full items-center gap-1.5 overflow-hidden px-1.5 text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring"
        title={[entry.task.title, entry.column.name, time_label, status_label]
          .filter(Boolean)
          .join(" · ")}
        onpointerup={(event) => event.currentTarget.blur()}
        onclick={() => entry_open(entry)}
        ondblclick={(event) => event.stopPropagation()}
      >
        <span
          class="h-3.5 w-1 shrink-0 rounded-full"
          style={`background-color: ${entry_color(entry)}`}
        ></span>
        {#if time_label}
          <span class="shrink-0 text-[10px] tabular-nums text-muted-foreground"
            >{time_label}</span
          >
        {/if}
        <span class={`min-w-0 flex-1 truncate ${task_title_class(entry)}`}
          >{entry.task.title}</span
        >
        <span class="flex shrink-0 text-muted-foreground">
          {@render task_status_icon(entry)}
        </span>
      </button>
    </CalendarTaskDraggable>
    {#if resizable}
      {@render resize_handle(entry, calendar_task_key(entry), "resize-start")}
      {@render resize_handle(entry, calendar_task_key(entry), "resize-end")}
    {/if}
    {#if !read_only}
      {@render task_menu_trigger(entry, resizable ? "right-2" : "right-0.5")}
    {/if}
  </div>
{/snippet}

{#snippet span_task(segment: CalendarSpanSegment, week: CalendarWeek)}
  {@const entry = segment.entry}
  {@const editable = entry_editable(entry)}
  {@const range_label = task_range_label(entry.task)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class={`group relative z-10 flex min-w-0 items-center transition-opacity ${
      is_drag_source(entry) ? "opacity-30" : ""
    } ${
      segment.continues_before ? "" : "ml-1.5"
    } ${segment.continues_after ? "" : "mr-1.5"}`}
    style={`grid-column: ${segment.column + 1} / span ${segment.length}; grid-row: ${segment.lane + 2};`}
    onpointerdowncapture={(event) => record_span_grab(event, segment, week)}
    ondblclick={(event) => event.stopPropagation()}
  >
    <CalendarTaskDraggable
      task_id={entry.task.id}
      instance_key={segment.key}
      data={drag_data(entry)}
      disabled={!editable}
      class={`flex min-w-0 flex-1 overflow-hidden text-xs ${task_card_class(entry)} ${
        segment.continues_before ? "rounded-l-none border-l-0" : "rounded-l-md"
      } ${segment.continues_after ? "rounded-r-none border-r-0" : "rounded-r-md"}`}
      oncontextmenu={read_only
        ? undefined
        : (event) => {
            event.preventDefault();
            if (editable) {
              onEditTask(entry.task);
            }
          }}
    >
      <button
        type="button"
        class="flex h-6 min-w-0 flex-1 items-center gap-1.5 overflow-hidden px-1.5 text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring"
        title={`${entry.task.title} · ${entry.column.name} · ${range_label}`}
        aria-label={m.calendar_span_label({
          title: entry.task.title,
          range: range_label,
        })}
        onpointerup={(event) => event.currentTarget.blur()}
        onclick={() => entry_open(entry)}
      >
        {#if segment.continues_before}
          <ChevronLeftIcon class="size-3 shrink-0 text-muted-foreground" />
        {:else}
          <span
            class="h-3.5 w-1 shrink-0 rounded-full"
            style={`background-color: ${entry_color(entry)}`}
          ></span>
        {/if}
        <span class={`min-w-0 shrink truncate ${task_title_class(entry)}`}
          >{entry.task.title}</span
        >
        <span
          class="min-w-0 flex-1 truncate text-[10px] tabular-nums text-muted-foreground"
          >{range_label}</span
        >
        <span class="flex shrink-0 text-muted-foreground">
          {@render task_status_icon(entry)}
        </span>
        {#if segment.continues_after}
          <ChevronRightIcon class="size-3 shrink-0 text-muted-foreground" />
        {/if}
      </button>
    </CalendarTaskDraggable>
    {#if editable && !segment.continues_before}
      {@render resize_handle(entry, segment.key, "resize-start")}
    {/if}
    {#if editable && !segment.continues_after}
      {@render resize_handle(entry, segment.key, "resize-end")}
    {/if}
    {#if !read_only}
      {@render task_menu_trigger(
        entry,
        segment.continues_after ? "right-0.5" : "right-2",
      )}
    {/if}
  </div>
{/snippet}

{#snippet ghost_bar(ghost: CalendarGhostSegment)}
  {@const preview = drop_preview}
  {#if preview?.dates}
    <div
      class={`pointer-events-none relative z-30 flex h-6.5 min-w-0 items-center gap-1.5 overflow-hidden border-2 border-dashed border-primary bg-[color-mix(in_oklab,var(--primary)_15%,var(--card))] px-1.5 text-xs shadow-sm self-center ${
        ghost.continues_before
          ? "rounded-l-none border-l-0"
          : "ml-1.5 rounded-l-md"
      } ${ghost.continues_after ? "rounded-r-none border-r-0" : "mr-1.5 rounded-r-md"}`}
      style={`grid-column: ${ghost.column + 1} / span ${ghost.length}; grid-row: ${ghost.lane + 2};`}
      aria-hidden="true"
    >
      <span class="min-w-0 shrink truncate font-medium"
        >{preview.task.title}</span
      >
      <span
        class="min-w-0 flex-1 truncate text-[10px] font-medium tabular-nums text-primary"
        >{task_range_label(preview.dates)}</span
      >
    </div>
  {/if}
{/snippet}

{#snippet ghost_card(preview: CalendarDropPreview)}
  {@const time_label = preview.dates
    ? task_time_label(preview.dates, preview.kind)
    : ""}
  <div
    class="pointer-events-none flex h-6.5 min-w-0 items-center gap-1.5 overflow-hidden rounded-md border-2 border-dashed border-primary bg-[color-mix(in_oklab,var(--primary)_15%,var(--card))] px-1.5 text-xs shadow-sm"
    aria-hidden="true"
  >
    {#if time_label}
      <span class="shrink-0 text-[10px] font-medium tabular-nums text-primary"
        >{time_label}</span
      >
    {/if}
    <span class="min-w-0 flex-1 truncate font-medium">{preview.task.title}</span
    >
  </div>
{/snippet}

{#snippet task_menu_trigger(entry: CalendarTask, position: string)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class={`absolute top-1/2 z-20 -translate-y-1/2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 ${position}`}
    onpointerdown={(event) => event.stopPropagation()}
    ondblclick={(event) => event.stopPropagation()}
  >
    {@render task_menu(entry)}
  </div>
{/snippet}

{#snippet resize_handle(
  entry: CalendarTask,
  key: string,
  mode: "resize-start" | "resize-end",
)}
  <CalendarTaskDraggable
    task_id={entry.task.id}
    instance_key={`${key}:${mode}`}
    data={drag_data(entry, mode)}
    aria-hidden={true}
    title={mode === "resize-start"
      ? m.calendar_resize_start()
      : m.calendar_resize_end()}
    class={`absolute inset-y-0 z-20 flex w-2 cursor-ew-resize touch-none items-center justify-center opacity-0 transition-opacity group-hover:opacity-100 ${
      mode === "resize-start" ? "-left-0.5" : "-right-0.5"
    }`}
  >
    <span class="h-3.5 w-1 rounded-full bg-foreground/50 hover:bg-foreground/80"
    ></span>
  </CalendarTaskDraggable>
{/snippet}

{#snippet hidden_tasks(day: CalendarDay, entries: CalendarTask[])}
  <Popover.Root>
    <Popover.Trigger>
      {#snippet child({ props })}
        <button
          {...props}
          type="button"
          class="rounded-sm px-1 text-[11px] font-medium text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          ondblclick={(event) => event.stopPropagation()}
        >
          {m.calendar_more_tasks({ count: entries.length })}
        </button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Content align="start" class="w-64 p-2">
      <p class="mb-1.5 px-1 text-xs font-medium text-muted-foreground">
        {formatDate(day.date, {
          year: "numeric",
          month: "long",
          day: "numeric",
        })}
      </p>
      <div class="flex max-h-64 flex-col gap-1 overflow-y-auto">
        {#each entries as entry (calendar_task_key(entry))}
          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            onclick={() => entry_open(entry)}
          >
            <span
              class="h-3 w-1 shrink-0 rounded-full"
              style={`background-color: ${entry_color(entry)}`}
            ></span>
            <span class="flex min-w-0 flex-1 flex-col">
              <span
                class={`truncate ${entry.archived ? "text-muted-foreground line-through" : entry.preview ? "text-muted-foreground" : "font-medium"}`}
                >{entry.task.title}</span
              >
              <span class="truncate text-[10px] text-muted-foreground"
                >{task_range_label(entry.task)}</span
              >
            </span>
          </button>
        {/each}
      </div>
    </Popover.Content>
  </Popover.Root>
{/snippet}
