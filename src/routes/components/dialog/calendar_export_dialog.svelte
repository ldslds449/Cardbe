<script lang="ts">
  import { getLocale, formatNumber } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import { logger } from "$lib/logger";
  import { tick, untrack } from "svelte";
  import { toast } from "svelte-sonner";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import SearchIcon from "@lucide/svelte/icons/search";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import type { Archive } from "../../type/archive.svelte";
  import type { Column } from "../../type/column.svelte";
  import MonthPicker from "../calendar/month_picker.svelte";
  import {
    build_calendar_days,
    type CalendarDay,
    type CalendarViewMode,
  } from "../calendar/calendar";
  import {
    DEFAULT_CALENDAR_EXPORT_OPTIONS,
    calendar_entry_key,
    estimate_calendar_export_async,
    render_calendar_export,
    save_calendar_export,
    save_split_calendar_png_export,
    selected_calendar_days,
    type CalendarExportFormat,
    type CalendarExportEstimate,
    type CalendarExportOptions,
    type CalendarExportPeriod,
    type CalendarExportTheme,
    type CalendarPngLayout,
    type CalendarPdfPagination,
    type CalendarPdfPaper,
  } from "../calendar/calendar-export";

  const PREFERENCES_KEY = "cardbe-calendar-export-preferences-v1";

  let {
    open = $bindable(),
    days,
    columns,
    archives,
    search_text,
    visible_date,
    today,
    show_archived,
    show_recurring_previews,
    view_mode,
    period_label,
  }: {
    open: boolean;
    days: CalendarDay[];
    columns: Column[];
    archives: Archive[];
    search_text: string;
    visible_date: Date;
    today: Date;
    show_archived: boolean;
    show_recurring_previews: boolean;
    view_mode: CalendarViewMode;
    period_label: string;
  } = $props();

  let search = $state("");
  let selected_entry_keys = $state<string[]>([]);
  let format = $state<CalendarExportFormat>("png");
  let png_layout = $state<CalendarPngLayout>("combined");
  let theme = $state<CalendarExportTheme>("light");
  let dpi = $state(150);
  let show_details = $state(true);
  let pdf_paper = $state<CalendarPdfPaper>("fit");
  let pdf_pagination = $state<CalendarPdfPagination>("single");
  let month_count = $state(1);
  let start_month = $state("");
  let end_month = $state("");
  let periods = $state<CalendarExportPeriod[]>([]);
  let known_entry_keys = $state<string[]>([]);
  let estimate = $state<CalendarExportEstimate>({
    width: 0,
    height: 0,
    megapixels: 0,
    memory_mb: 0,
    page_count: 0,
    too_large: false,
  });
  let preparing_periods = $state(false);
  let calculating_estimate = $state(false);
  let exporting = $state(false);
  let export_progress = $state("");
  let initialized_for_open = false;
  let period_generation = 0;
  let estimate_generation = 0;
  let period_timer: ReturnType<typeof setTimeout> | undefined;
  let estimate_timer: ReturnType<typeof setTimeout> | undefined;

  function current_options(): CalendarExportOptions {
    return {
      theme,
      dpi,
      show_title: true,
      show_details,
      pdf_paper,
      pdf_pagination,
      png_layout,
    };
  }

  function load_preferences(): Partial<CalendarExportOptions> & {
    format?: CalendarExportFormat;
    month_count?: number;
  } {
    try {
      return JSON.parse(localStorage.getItem(PREFERENCES_KEY) ?? "{}");
    } catch (error) {
      logger.debug("calendar.preferences_load.failed", { error });
      return {};
    }
  }

  function save_preferences() {
    try {
      localStorage.setItem(
        PREFERENCES_KEY,
        JSON.stringify({ format, month_count, ...current_options() }),
      );
    } catch (error) {
      logger.debug("calendar.preferences_save.failed", { error });
      // Exporting still works when storage is unavailable.
    }
  }

  function month_value(date: Date): string {
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}`;
  }

  function parsed_month(value: string, fallback: Date): Date {
    const match = /^(\d{4})-(\d{2})$/.exec(value);
    if (!match) {
      return new Date(fallback.getFullYear(), fallback.getMonth(), 1);
    }
    const month = Number(match[2]);
    if (month < 1 || month > 12) {
      return new Date(fallback.getFullYear(), fallback.getMonth(), 1);
    }
    return new Date(Number(match[1]), month - 1, 1);
  }

  function normalized_month_range(): { start: Date; end: Date; count: number } {
    const fallback = new Date(
      visible_date.getFullYear(),
      visible_date.getMonth(),
      1,
    );
    const start = parsed_month(start_month, fallback);
    let end = parsed_month(end_month, start);
    let difference =
      (end.getFullYear() - start.getFullYear()) * 12 +
      end.getMonth() -
      start.getMonth();
    if (difference < 0) {
      end = new Date(start);
      difference = 0;
    } else if (difference > 11) {
      end = new Date(start.getFullYear(), start.getMonth() + 11, 1);
      difference = 11;
    }
    return { start, end, count: difference + 1 };
  }

  function build_export_period(
    index: number,
    count: number,
  ): CalendarExportPeriod {
    if (view_mode !== "month") {
      return { label: period_label, days };
    }
    const start = parsed_month(start_month, visible_date);
    const month = new Date(start.getFullYear(), start.getMonth() + index, 1);
    const uses_visible_month =
      month.getFullYear() === visible_date.getFullYear() &&
      month.getMonth() === visible_date.getMonth();
    const month_days =
      index === 0 && count === 1 && uses_visible_month
        ? days
        : build_calendar_days(
            columns,
            month,
            today,
            search_text,
            archives,
            show_archived,
            show_recurring_previews,
            "month",
          ).map((day) => (day.in_current_month ? day : { ...day, tasks: [] }));
    return {
      label: month.toLocaleDateString(getLocale(), {
        year: "numeric",
        month: "long",
      }),
      days: month_days,
    };
  }

  function period_entry_keys(periods: CalendarExportPeriod[]): string[] {
    return periods.flatMap((period) =>
      period.days.flatMap((day) =>
        day.tasks.map((entry) => calendar_entry_key(day, entry)),
      ),
    );
  }

  function initialize() {
    const stored = load_preferences();
    const default_theme: CalendarExportTheme =
      document.documentElement.classList.contains("dark") ? "dark" : "light";
    search = "";
    format = stored.format === "pdf" ? "pdf" : "png";
    png_layout = stored.png_layout === "separate" ? "separate" : "combined";
    theme =
      stored.theme === "dark" || stored.theme === "light"
        ? stored.theme
        : default_theme;
    dpi = Math.min(
      300,
      Math.max(72, Number(stored.dpi) || DEFAULT_CALENDAR_EXPORT_OPTIONS.dpi),
    );
    show_details = stored.show_details ?? true;
    pdf_paper =
      stored.pdf_paper === "a4" || stored.pdf_paper === "a3"
        ? stored.pdf_paper
        : "fit";
    pdf_pagination = stored.pdf_pagination === "weeks" ? "weeks" : "single";
    month_count =
      view_mode === "month"
        ? Math.min(12, Math.max(1, Math.round(Number(stored.month_count) || 1)))
        : 1;
    const initial_start = new Date(
      visible_date.getFullYear(),
      visible_date.getMonth(),
      1,
    );
    start_month = month_value(initial_start);
    end_month = month_value(
      new Date(
        initial_start.getFullYear(),
        initial_start.getMonth() + month_count - 1,
        1,
      ),
    );
    periods = [build_export_period(0, 1)];
    const initial_keys = period_entry_keys(periods);
    selected_entry_keys = initial_keys;
    known_entry_keys = initial_keys;
    void rebuild_export_periods();
  }

  $effect(() => {
    if (open && !initialized_for_open) {
      initialize();
      initialized_for_open = true;
    } else if (!open && initialized_for_open) {
      save_preferences();
      period_generation += 1;
      estimate_generation += 1;
      clearTimeout(period_timer);
      clearTimeout(estimate_timer);
      preparing_periods = false;
      calculating_estimate = false;
      initialized_for_open = false;
    }
  });

  $effect(() => {
    getLocale();
    if (open && initialized_for_open) {
      untrack(() => schedule_period_rebuild());
    }
  });

  const all_days = $derived(periods.flatMap((period) => period.days));
  const all_entry_keys = $derived(period_entry_keys(periods));
  const valid_selected_entry_keys = $derived.by(() => {
    const available = new Set(all_entry_keys);
    return selected_entry_keys.filter((key) => available.has(key));
  });
  const selected_periods = $derived(
    periods.map((period) => ({
      ...period,
      days: selected_calendar_days(period.days, valid_selected_entry_keys),
    })),
  );
  const export_title = $derived(
    periods.length === 1 ? (periods[0]?.label ?? period_label) : "",
  );
  const default_filename = $derived.by(() => {
    if (periods.length > 1) {
      return `${periods[0].label} - ${periods.at(-1)?.label}`;
    }
    return periods[0]?.label ?? period_label;
  });
  const preview_days = $derived((selected_periods[0]?.days ?? []).slice(0, 7));
  const preview_entry = $derived(preview_days.flatMap((day) => day.tasks)[0]);

  const days_with_entries = $derived(
    all_days.filter((day) => day.tasks.length > 0),
  );
  const visible_days = $derived.by(() => {
    const needle = search.trim().toLocaleLowerCase();
    if (!needle) {
      return days_with_entries;
    }
    return days_with_entries
      .map((day) => ({
        ...day,
        tasks: day.tasks.filter((entry) =>
          [
            entry.task.title,
            entry.task.description,
            entry.column.name,
            ...entry.task.labels,
          ]
            .join(" ")
            .toLocaleLowerCase()
            .includes(needle),
        ),
      }))
      .filter((day) => day.tasks.length > 0);
  });
  const visible_entry_keys = $derived(
    visible_days.flatMap((day) =>
      day.tasks.map((entry) => calendar_entry_key(day, entry)),
    ),
  );
  const all_visible_selected = $derived(
    visible_entry_keys.length > 0 &&
      visible_entry_keys.every((key) => selected_entry_keys.includes(key)),
  );
  const no_visible_selected = $derived(
    visible_entry_keys.every((key) => !selected_entry_keys.includes(key)),
  );

  $effect(() => {
    if (!open || preparing_periods) {
      return;
    }
    const requested_periods = selected_periods;
    const requested_title = export_title;
    const requested_format = format;
    const requested_options = current_options();
    const generation = ++estimate_generation;
    clearTimeout(estimate_timer);
    calculating_estimate = true;
    estimate_timer = setTimeout(() => {
      void estimate_calendar_export_async(
        requested_periods,
        requested_title,
        requested_format,
        requested_options,
      )
        .then((result) => {
          if (generation === estimate_generation && open) {
            estimate = result;
          }
        })
        .catch((error) => {
          logger.warn("calendar.export_estimate.failed", error);
          console.error(m.ui_couldn_t_estimate_calendar_export(), error);
        })
        .finally(() => {
          if (generation === estimate_generation) {
            calculating_estimate = false;
          }
        });
    }, 120);
  });

  function set_entry_selected(key: string, checked: boolean) {
    selected_entry_keys = checked
      ? Array.from(new Set([...selected_entry_keys, key]))
      : selected_entry_keys.filter((candidate) => candidate !== key);
  }

  function set_day_selected(day: CalendarDay, checked: boolean) {
    const keys = day.tasks.map((entry) => calendar_entry_key(day, entry));
    const key_set = new Set(keys);
    selected_entry_keys = checked
      ? Array.from(new Set([...selected_entry_keys, ...keys]))
      : selected_entry_keys.filter((key) => !key_set.has(key));
  }

  function set_visible_selected(checked: boolean) {
    const keys = new Set(visible_entry_keys);
    selected_entry_keys = checked
      ? Array.from(new Set([...selected_entry_keys, ...keys]))
      : selected_entry_keys.filter((key) => !keys.has(key));
  }

  function format_time(date: Date | undefined): string {
    if (
      !date ||
      (date.getHours() === 0 &&
        date.getMinutes() === 0 &&
        date.getSeconds() === 0)
    ) {
      return "";
    }
    return date.toLocaleTimeString(getLocale(), {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  }

  function set_dpi() {
    dpi = Math.min(300, Math.max(72, Math.round(Number(dpi) || 96)));
  }

  function next_ui_frame(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()));
  }

  async function rebuild_export_periods() {
    const generation = ++period_generation;
    estimate_generation += 1;
    clearTimeout(estimate_timer);
    calculating_estimate = false;
    preparing_periods = true;
    const range = normalized_month_range();
    start_month = month_value(range.start);
    end_month = month_value(range.end);
    month_count = view_mode === "month" ? range.count : 1;
    const count = month_count;
    await tick();
    await next_ui_frame();

    const next_periods: CalendarExportPeriod[] = [];
    for (let index = 0; index < count; index += 1) {
      if (generation !== period_generation || !open) {
        return;
      }
      next_periods.push(build_export_period(index, count));
      if (index < count - 1) {
        await next_ui_frame();
      }
    }
    if (generation !== period_generation || !open) {
      return;
    }

    const available = period_entry_keys(next_periods);
    const known = new Set(known_entry_keys);
    selected_entry_keys = Array.from(
      new Set([
        ...selected_entry_keys,
        ...available.filter((key) => !known.has(key)),
      ]),
    );
    known_entry_keys = Array.from(new Set([...known_entry_keys, ...available]));
    periods = next_periods;
    preparing_periods = false;
  }

  function schedule_period_rebuild() {
    period_generation += 1;
    estimate_generation += 1;
    clearTimeout(period_timer);
    clearTimeout(estimate_timer);
    calculating_estimate = false;
    preparing_periods = true;
    period_timer = setTimeout(() => void rebuild_export_periods(), 160);
  }

  async function export_calendar() {
    if (
      exporting ||
      preparing_periods ||
      calculating_estimate ||
      valid_selected_entry_keys.length === 0 ||
      estimate.too_large
    ) {
      return;
    }
    exporting = true;
    export_progress = "";
    save_preferences();
    await tick();
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => resolve()),
    );
    try {
      if (
        format === "png" &&
        png_layout === "separate" &&
        selected_periods.length > 1
      ) {
        const count = await save_split_calendar_png_export(
          selected_periods,
          export_title,
          current_options(),
          (completed, total) => {
            export_progress =
              completed === 0
                ? m.export_preparing_images({ total })
                : m.export_saving_images({ completed, total });
          },
        );
        if (count !== null) {
          toast.success(m.export_png_success({ count }));
          open = false;
        }
        return;
      }
      const blob = await render_calendar_export(
        selected_periods,
        export_title,
        format,
        current_options(),
      );
      if (await save_calendar_export(blob, default_filename, format)) {
        toast.success(m.export_success({ format: format.toUpperCase() }));
        open = false;
      }
    } catch (error) {
      logger.error("calendar.export.failed", error);
      console.error(m.ui_couldn_t_export_calendar(), error);
      toast.error(
        error instanceof Error &&
          [
            m.export_canvas_error(),
            m.export_dpi_error(),
            m.export_create_error(),
            m.export_png_data_error(),
            m.export_png_chunk_error(),
            m.export_png_structure_error(),
            m.export_pdf_color_error(),
            m.export_pdf_page_error(),
            m.export_font_empty(),
            m.export_font_error(),
            m.export_select_month(),
            m.export_large_error(),
            m.export_filename_error(),
          ].some((message) => message === error.message)
          ? error.message
          : m.ui_couldn_t_export_the_calendar(),
      );
    } finally {
      exporting = false;
      export_progress = "";
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="flex h-[94vh] max-h-[94vh] flex-col gap-0 overflow-hidden p-0 sm:h-[min(94vh,56rem)] sm:max-w-3xl"
  >
    <Dialog.Header class="shrink-0 px-6 pb-4 pt-6">
      <Dialog.Title>{m.ui_export_calendar()}</Dialog.Title>
      <Dialog.Description>
        {m.export_description()}
      </Dialog.Description>
    </Dialog.Header>

    <div
      class="grid min-h-0 flex-1 gap-4 overflow-y-auto overscroll-contain px-6 pb-6 pt-2"
    >
      {#if view_mode === "month"}
        <div
          class="flex flex-wrap items-end justify-between gap-3 rounded-lg border bg-muted/20 p-3"
        >
          <div class="grid flex-1 gap-3 sm:grid-cols-2">
            <label class="grid gap-1.5 text-sm font-medium">
              {m.ui_start_month()}
              <MonthPicker
                bind:value={start_month}
                onValueChange={schedule_period_rebuild}
                disabled={exporting}
              />
            </label>
            <label class="grid gap-1.5 text-sm font-medium">
              {m.ui_end_month()}
              <MonthPicker
                bind:value={end_month}
                onValueChange={schedule_period_rebuild}
                disabled={exporting}
              />
            </label>
          </div>
          <p class="pb-2 text-xs text-muted-foreground">
            {m.export_month_count({ count: month_count })}
            {m.export_max_months()}
          </p>
        </div>
      {/if}

      <div class="grid gap-4 rounded-lg border bg-muted/20 p-3 sm:grid-cols-3">
        <fieldset class="grid gap-2">
          <legend class="text-sm font-medium">{m.ui_format()}</legend>
          <div
            class="flex h-9 items-stretch overflow-hidden rounded-md border bg-background"
          >
            <Button
              variant={format === "png" ? "secondary" : "ghost"}
              size="sm"
              class="h-full flex-1 rounded-none border-0 border-r shadow-none focus-visible:z-10"
              aria-pressed={format === "png"}
              onclick={() => {
                format = "png";
              }}>PNG</Button
            >
            <Button
              variant={format === "pdf" ? "secondary" : "ghost"}
              size="sm"
              class="h-full flex-1 rounded-none border-0 shadow-none focus-visible:z-10"
              aria-pressed={format === "pdf"}
              onclick={() => {
                format = "pdf";
              }}>PDF</Button
            >
          </div>
        </fieldset>
        <fieldset class="grid gap-2">
          <legend class="text-sm font-medium">{m.ui_theme()}</legend>
          <div
            class="flex h-9 items-stretch overflow-hidden rounded-md border bg-background"
          >
            <Button
              variant={theme === "light" ? "secondary" : "ghost"}
              size="sm"
              class="h-full flex-1 rounded-none border-0 border-r shadow-none focus-visible:z-10"
              aria-pressed={theme === "light"}
              onclick={() => {
                theme = "light";
              }}>{m.ui_light()}</Button
            >
            <Button
              variant={theme === "dark" ? "secondary" : "ghost"}
              size="sm"
              class="h-full flex-1 rounded-none border-0 shadow-none focus-visible:z-10"
              aria-pressed={theme === "dark"}
              onclick={() => {
                theme = "dark";
              }}>{m.ui_dark()}</Button
            >
          </div>
        </fieldset>
        {#if format === "png"}
          <label class="grid gap-2 text-sm font-medium">
            DPI
            <Input
              type="number"
              min="72"
              max="300"
              step="1"
              bind:value={dpi}
              onfocusout={set_dpi}
            />
          </label>
        {/if}

        {#if format === "png" && view_mode === "month"}
          <fieldset class="grid gap-2 sm:col-span-3">
            <legend class="text-sm font-medium">{m.ui_png_layout()}</legend>
            <div
              class="flex h-9 items-stretch overflow-hidden rounded-md border bg-background"
            >
              <Button
                variant={png_layout === "combined" ? "secondary" : "ghost"}
                size="sm"
                class="h-full flex-1 rounded-none border-0 border-r shadow-none focus-visible:z-10"
                aria-pressed={png_layout === "combined"}
                onclick={() => {
                  png_layout = "combined";
                }}>{m.ui_single_image()}</Button
              >
              <Button
                variant={png_layout === "separate" ? "secondary" : "ghost"}
                size="sm"
                class="h-full flex-1 rounded-none border-0 shadow-none focus-visible:z-10"
                aria-pressed={png_layout === "separate"}
                onclick={() => {
                  png_layout = "separate";
                }}>{m.ui_separate_images()}</Button
              >
            </div>
            {#if png_layout === "separate"}
              <p class="text-xs text-muted-foreground">
                {m.ui_saves_one_png_per_month_in_a_folder_you_choose()}
              </p>
            {/if}
          </fieldset>
        {/if}

        {#if format === "pdf"}
          <fieldset class="grid gap-2 sm:col-span-2">
            <legend class="text-sm font-medium">{m.ui_pdf_page_size()}</legend>
            <div
              class="flex h-9 items-stretch overflow-hidden rounded-md border bg-background"
            >
              {#each [["fit", m.ui_fit_content()], ["a4", m.export_a4_landscape()], ["a3", m.export_a3_landscape()]] as [value, label]}
                <Button
                  variant={pdf_paper === value ? "secondary" : "ghost"}
                  size="sm"
                  class="h-full flex-1 rounded-none border-0 border-r shadow-none last:border-r-0 focus-visible:z-10"
                  aria-pressed={pdf_paper === value}
                  onclick={() => {
                    pdf_paper = value as CalendarPdfPaper;
                  }}>{label}</Button
                >
              {/each}
            </div>
          </fieldset>
          <fieldset class="grid gap-2">
            <legend class="text-sm font-medium">{m.ui_pagination()}</legend>
            <div
              class="flex h-9 items-stretch overflow-hidden rounded-md border bg-background"
            >
              <Button
                variant={pdf_pagination === "single" ? "secondary" : "ghost"}
                size="sm"
                class="h-full flex-1 rounded-none border-0 border-r shadow-none focus-visible:z-10"
                aria-pressed={pdf_pagination === "single"}
                onclick={() => {
                  pdf_pagination = "single";
                }}
                >{view_mode === "month"
                  ? m.ui_months()
                  : m.ui_one_page()}</Button
              >
              <Button
                variant={pdf_pagination === "weeks" ? "secondary" : "ghost"}
                size="sm"
                class="h-full flex-1 rounded-none border-0 shadow-none focus-visible:z-10"
                aria-pressed={pdf_pagination === "weeks"}
                onclick={() => {
                  pdf_pagination = "weeks";
                }}>{m.ui_weeks()}</Button
              >
            </div>
          </fieldset>
        {/if}

        <div class="grid gap-2 sm:col-span-3">
          <label class="flex cursor-pointer items-center gap-2 text-sm">
            <Checkbox
              checked={show_details}
              onCheckedChange={(checked) => {
                show_details = checked === true;
              }}
            />
            {m.ui_show_entry_count_and_date()}
          </label>
        </div>
      </div>

      <div
        class={`rounded-lg border p-3 ${estimate.too_large ? "border-destructive/50 bg-destructive/5" : "bg-muted/20"}`}
      >
        {#if preparing_periods || calculating_estimate}
          <div
            class="flex min-h-5 items-center gap-2 text-xs text-muted-foreground"
            role="status"
          >
            <LoaderCircleIcon class="size-3.5 animate-spin" />
            {m.ui_calculating_output_size()}
          </div>
        {:else}
          <div
            class="flex flex-wrap items-center justify-between gap-2 text-xs"
          >
            {#if format === "pdf"}
              <span class="text-muted-foreground"
                >{m.ui_vector_pdf_scalable_text_and_graphics()}</span
              >
              <span class="text-muted-foreground"
                >{m.export_page_count({ count: estimate.page_count })}</span
              >
            {:else}
              <span
                class={estimate.too_large
                  ? "font-medium text-destructive"
                  : "text-muted-foreground"}
              >
                {estimate.width.toLocaleString(getLocale())}
                × {estimate.height.toLocaleString(getLocale())} px ·
                {formatNumber(estimate.megapixels, {
                  minimumFractionDigits: 1,
                  maximumFractionDigits: 1,
                })}
                MP · ~{formatNumber(estimate.memory_mb, {
                  maximumFractionDigits: 0,
                })}
                {m.export_canvas_memory()}
              </span>
              {#if png_layout === "separate"}
                <span class="text-muted-foreground"
                  >{m.export_image_count({ count: estimate.page_count })}</span
                >
              {/if}
            {/if}
          </div>
        {/if}
        {#if !preparing_periods && !calculating_estimate && estimate.too_large}
          <p class="mt-1 text-xs text-destructive">
            {format === "pdf"
              ? "A PDF page is too large. Lower the DPI or export one page per week."
              : png_layout === "separate"
                ? m.ui_a_monthly_image_is_too_large_lower_the_dpi_hide_empty_first_last_weeks_or_use_pdf()
                : m.ui_output_is_too_large_lower_the_dpi_hide_empty_first_last_weeks_use_separate_png_images_or_use_pdf()}
          </p>
        {/if}
      </div>

      <div
        class="relative isolate shrink-0 rounded-lg border p-3 shadow-inner"
        style={`background:${theme === "dark" ? "#0f172a" : "#f8fafc"};color:${theme === "dark" ? "#f8fafc" : "#0f172a"};`}
        aria-label={m.ui_export_preview()}
        aria-busy={preparing_periods || calculating_estimate}
      >
        {#if preparing_periods || calculating_estimate}
          <div
            class="absolute inset-0 z-10 flex items-center justify-center rounded-lg bg-background/70 backdrop-blur-[1px]"
            role="status"
          >
            <span
              class="flex items-center gap-2 rounded-full border bg-background px-3 py-1.5 text-xs text-foreground shadow-sm"
            >
              <LoaderCircleIcon class="size-4 animate-spin" />
              {m.ui_updating_preview()}
            </span>
          </div>
        {/if}
        <div class="mb-2 flex items-end justify-between gap-3">
          <div class="min-w-0">
            <div class="break-words text-sm font-bold leading-tight">
              {periods[0]?.label ?? period_label}
            </div>
            {#if show_details}
              <div class="mt-0.5 text-[10px] opacity-60">
                {m.export_entries_date({
                  count: valid_selected_entry_keys.length,
                  date: new Date().toLocaleDateString(getLocale()),
                })}
              </div>
            {/if}
          </div>
          <span class="shrink-0 text-[10px] uppercase tracking-wide opacity-50"
            >{m.common_preview()}</span
          >
        </div>
        <div
          class="grid grid-cols-7 gap-px overflow-hidden rounded border border-current/15 bg-current/15"
        >
          {#each preview_days as day (day.key)}
            <div
              class="min-h-16 bg-[var(--preview-day)] p-1"
              style={`--preview-day:${theme === "dark" ? "#111827" : "#ffffff"}`}
            >
              <div class="text-[9px] font-semibold opacity-65">
                {day.date.getDate()}
              </div>
              {#if day.tasks.length > 0}
                <div
                  class="mt-1 line-clamp-3 break-words rounded border border-current/15 px-1 py-0.5 text-[8px] leading-tight"
                >
                  {day.tasks[0].task.title || m.task_untitled()}
                </div>
              {/if}
            </div>
          {/each}
        </div>
        {#if preview_entry}
          <div class="mt-2 truncate text-[9px] opacity-55">
            {m.export_sample()}
            {preview_entry.column.name}
          </div>
        {/if}
      </div>

      <div class="grid gap-2">
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="text-sm font-medium">{m.ui_entries_to_include()}</div>
            <div class="text-xs text-muted-foreground">
              {m.export_entries_selected({
                selected: valid_selected_entry_keys.length,
                total: all_entry_keys.length,
              })}
            </div>
          </div>
        </div>
        <div class="relative">
          <SearchIcon
            class="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground"
          />
          <Input
            class="pl-9"
            bind:value={search}
            placeholder={m.ui_search_tasks_columns_descriptions_or_labels()}
            aria-label={m.ui_search_calendar_entries()}
          />
        </div>
        <div class="flex items-center justify-between gap-3">
          <span class="text-xs tabular-nums text-muted-foreground"
            >{search.trim()
              ? m.export_matching_entries({ count: visible_entry_keys.length })
              : m.export_available_entries({
                  count: visible_entry_keys.length,
                })}</span
          >
          <div class="flex gap-1">
            <Button
              variant="ghost"
              size="sm"
              disabled={visible_entry_keys.length === 0 || all_visible_selected}
              onclick={() => set_visible_selected(true)}
              >{m.ui_select_all()}</Button
            >
            <Button
              variant="ghost"
              size="sm"
              disabled={visible_entry_keys.length === 0 || no_visible_selected}
              onclick={() => set_visible_selected(false)}
              >{m.ui_clear_all()}</Button
            >
          </div>
        </div>

        <div class="max-h-80 space-y-2 overflow-y-auto rounded-md border p-2">
          {#each visible_days as day (day.key)}
            {@const day_keys = day.tasks.map((entry) =>
              calendar_entry_key(day, entry),
            )}
            {@const selected_in_day = day_keys.filter((key) =>
              selected_entry_keys.includes(key),
            ).length}
            <div class="rounded-md border bg-muted/20">
              <label
                class="flex cursor-pointer items-center gap-3 rounded-t-md px-3 py-2 hover:bg-muted/60"
              >
                <Checkbox
                  checked={selected_in_day === day_keys.length}
                  indeterminate={selected_in_day > 0 &&
                    selected_in_day < day_keys.length}
                  onCheckedChange={(checked) =>
                    set_day_selected(day, checked === true)}
                />
                <span class="min-w-0 flex-1 truncate text-sm font-medium"
                  >{day.date.toLocaleDateString(getLocale(), {
                    weekday: "short",
                    year: "numeric",
                    month: "short",
                    day: "numeric",
                  })}</span
                >
                <span class="text-xs tabular-nums text-muted-foreground"
                  >{selected_in_day}/{formatNumber(day_keys.length)}</span
                >
              </label>
              <div class="border-t px-2 py-1">
                {#each day.tasks as entry (calendar_entry_key(day, entry))}
                  {@const key = calendar_entry_key(day, entry)}
                  <label
                    class="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 hover:bg-muted/60"
                  >
                    <Checkbox
                      class="mt-0.5"
                      checked={selected_entry_keys.includes(key)}
                      onCheckedChange={(checked) =>
                        set_entry_selected(key, checked === true)}
                    />
                    <span class="min-w-0 flex-1">
                      <span class="block truncate text-sm"
                        >{#if format_time(entry.task.due_time)}
                          <span class="mr-1 text-muted-foreground"
                            >{format_time(entry.task.due_time)}</span
                          >
                        {/if}
                        {entry.task.title || m.task_untitled()}</span
                      >
                      <span class="block truncate text-xs text-muted-foreground"
                        >{entry.column.name}{entry.archived
                          ? " · Archived"
                          : entry.preview
                            ? " · Recurring preview"
                            : ""}</span
                      >
                    </span>
                  </label>
                {/each}
              </div>
            </div>
          {:else}
            <p class="p-5 text-center text-sm text-muted-foreground">
              {m.ui_no_matching_calendar_entries()}
            </p>
          {/each}
        </div>
      </div>
    </div>

    <Dialog.Footer
      class="relative z-20 shrink-0 border-t bg-background px-6 py-4"
    >
      <Dialog.Close>
        {#snippet child({ props })}
          <Button {...props} variant="outline" disabled={exporting}
            >{m.common_cancel()}</Button
          >
        {/snippet}
      </Dialog.Close>
      <Button
        disabled={exporting ||
          preparing_periods ||
          calculating_estimate ||
          valid_selected_entry_keys.length === 0 ||
          estimate.too_large}
        onclick={() => void export_calendar()}
      >
        {#if exporting}
          <LoaderCircleIcon class="animate-spin" />
        {:else}
          <DownloadIcon />
        {/if}
        {exporting
          ? export_progress || m.ui_rendering()
          : preparing_periods || calculating_estimate
            ? m.ui_preparing()
            : format === "png" &&
                png_layout === "separate" &&
                selected_periods.length > 1
              ? m.ui_export_pngs()
              : m.export_action({ format: format.toUpperCase() })}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
