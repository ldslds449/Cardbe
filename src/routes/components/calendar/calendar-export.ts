import { join } from "@tauri-apps/api/path";
import { open, save } from "@tauri-apps/plugin-dialog";
import { exists, writeFile } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import type { Color, PDFFont, PDFPage } from "pdf-lib";
import type { CalendarDay, CalendarTask, CalendarViewMode } from "./calendar";

type FontkitModule = typeof import("@pdf-lib/fontkit");
type FontkitFont = ReturnType<FontkitModule["create"]>;
type FontkitCollection = { fonts: FontkitFont[] };
type PdfRgb = typeof import("pdf-lib").rgb;

let pdf_rgb: PdfRgb | undefined;

export type CalendarExportTheme = "light" | "dark";
export type CalendarExportFormat = "png" | "pdf";
export type CalendarPngLayout = "combined" | "separate";
export type CalendarPdfPaper = "fit" | "a4" | "a3";
export type CalendarPdfPagination = "single" | "weeks";

export interface CalendarExportOptions {
  theme: CalendarExportTheme;
  dpi: number;
  show_title: boolean;
  show_details: boolean;
  pdf_paper: CalendarPdfPaper;
  pdf_pagination: CalendarPdfPagination;
  png_layout: CalendarPngLayout;
}

export interface CalendarExportEstimate {
  width: number;
  height: number;
  megapixels: number;
  memory_mb: number;
  page_count: number;
  too_large: boolean;
}

export interface CalendarExportPeriod {
  label: string;
  days: CalendarDay[];
}

const CELL_WIDTH = 252;
const PAGE_PADDING = 48;
const WEEKDAY_HEIGHT = 44;
const DAY_HEADER_HEIGHT = 48;
const TASK_GAP = 7;
const MIN_ROW_HEIGHT = 190;
const LOGICAL_DPI = 96;
const MAX_CANVAS_DIMENSION = 16_384;
const MAX_CANVAS_AREA = 30_000_000;
const PDF_MARGIN = 24;
const EXPORT_LOCALE = "en-US";

export const DEFAULT_CALENDAR_EXPORT_OPTIONS: CalendarExportOptions = {
  theme: "light",
  dpi: 150,
  show_title: true,
  show_details: true,
  pdf_paper: "fit",
  pdf_pagination: "single",
  png_layout: "combined",
};

const PALETTES = {
  light: {
    page: "#f8fafc", text: "#0f172a", secondary_text: "#64748b",
    weekday: "#e2e8f0", weekday_text: "#475569", day: "#ffffff",
    outside_day: "#f1f5f9", outside_text: "#94a3b8", border: "#cbd5e1",
    card: "#ffffff", subdued_card: "#f1f5f9", preview_card: "#f8fafc",
    today: "#0f172a", today_text: "#ffffff",
  },
  dark: {
    page: "#0f172a", text: "#f8fafc", secondary_text: "#94a3b8",
    weekday: "#1e293b", weekday_text: "#cbd5e1", day: "#111827",
    outside_day: "#172033", outside_text: "#64748b", border: "#334155",
    card: "#1e293b", subdued_card: "#202a3b", preview_card: "#172033",
    today: "#f8fafc", today_text: "#0f172a",
  },
} as const;

interface TaskLayout {
  entry: CalendarTask;
  title_lines: string[];
  height: number;
}

interface MeasuredCalendar {
  title_lines: string[];
  layouts: Map<CalendarTask, TaskLayout>;
  row_heights: number[];
  logical_width: number;
  logical_height: number;
  grid_top: number;
}

interface CalendarCanvasResult {
  canvas: HTMLCanvasElement;
  pixel_width: number;
  pixel_height: number;
  dpi: number;
}

export function calendar_entry_key(day: CalendarDay, entry: CalendarTask): string {
  return [
    day.key,
    entry.task.id,
    entry.task.due_time?.getTime() ?? 0,
    entry.archived ? "archived" : "active",
    entry.preview ? "preview" : "actual",
  ].join(":");
}

export function selected_calendar_days(
  days: CalendarDay[],
  selected_entry_keys: Iterable<string>,
): CalendarDay[] {
  const selected = new Set(selected_entry_keys);
  return days.map((day) => ({
    ...day,
    tasks: day.tasks.filter((entry) => selected.has(calendar_entry_key(day, entry))),
  }));
}

function rounded_rect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  context.beginPath();
  if (typeof context.roundRect === "function") {
    context.roundRect(x, y, width, height, radius);
    return;
  }
  const r = Math.min(radius, width / 2, height / 2);
  context.moveTo(x + r, y);
  context.arcTo(x + width, y, x + width, y + height, r);
  context.arcTo(x + width, y + height, x, y + height, r);
  context.arcTo(x, y + height, x, y, r);
  context.arcTo(x, y, x + width, y, r);
  context.closePath();
}

function fit_text(context: CanvasRenderingContext2D, value: string, max_width: number): string {
  if (context.measureText(value).width <= max_width) return value;
  let low = 0;
  let high = value.length;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (context.measureText(`${value.slice(0, middle)}…`).width <= max_width) low = middle;
    else high = middle - 1;
  }
  return `${value.slice(0, low)}…`;
}

export function wrap_calendar_text(
  value: string,
  max_width: number,
  measure: (value: string) => number,
): string[] {
  const lines: string[] = [];
  for (const paragraph of (value || " ").split("\n")) {
    let current = "";
    for (const character of Array.from(paragraph || " ")) {
      const candidate = `${current}${character}`;
      if (!current || measure(candidate) <= max_width) {
        current = candidate;
        continue;
      }

      const break_at = current.lastIndexOf(" ");
      if (break_at > 0) {
        lines.push(current.slice(0, break_at));
        current = current.slice(break_at + 1);
        if (measure(`${current}${character}`) > max_width) {
          lines.push(current);
          current = character;
        } else {
          current += character;
        }
      } else {
        lines.push(current);
        current = character;
      }
    }
    lines.push(current.trimEnd() || " ");
  }
  return lines;
}

function wrap_text(context: CanvasRenderingContext2D, value: string, max_width: number): string[] {
  return wrap_calendar_text(value, max_width, (candidate) => context.measureText(candidate).width);
}

function has_time(date: Date): boolean {
  return date.getHours() !== 0 || date.getMinutes() !== 0 || date.getSeconds() !== 0;
}

export function calendar_export_accent(color: string, theme: CalendarExportTheme): string {
  const match = color.match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/i);
  if (!match) return color || (theme === "dark" ? "#94a3b8" : "#64748b");
  const value = match[1].length === 3
    ? match[1].split("").map((part) => `${part}${part}`).join("")
    : match[1];
  const channels = [0, 2, 4].map((offset) => Number.parseInt(value.slice(offset, offset + 2), 16));
  const luminance = (channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722) / 255;
  if (theme === "dark" && luminance < 0.28) {
    return `rgb(${channels.map((channel) => Math.round(channel + (255 - channel) * 0.5)).join(", ")})`;
  }
  if (theme === "light" && luminance > 0.88) {
    return `rgb(${channels.map((channel) => Math.round(channel * 0.62)).join(", ")})`;
  }
  return color;
}

function task_title(entry: CalendarTask): string {
  const due_time = entry.task.due_time;
  const time = due_time && has_time(due_time)
    ? `${due_time.toLocaleTimeString(EXPORT_LOCALE, { hour: "2-digit", minute: "2-digit", hour12: false })}  `
    : "";
  return `${time}${entry.task.title || "Untitled task"}`;
}

function build_task_layouts(
  context: CanvasRenderingContext2D,
  days: CalendarDay[],
): Map<CalendarTask, TaskLayout> {
  const layouts = new Map<CalendarTask, TaskLayout>();
  context.font = "600 14px system-ui, sans-serif";
  for (const day of days) {
    for (const entry of day.tasks) {
      const title_lines = wrap_text(context, task_title(entry), CELL_WIDTH - 47);
      layouts.set(entry, { entry, title_lines, height: title_lines.length * 18 + 30 });
    }
  }
  return layouts;
}

function measure_calendar(
  context: CanvasRenderingContext2D,
  days: CalendarDay[],
  title: string,
  options: Pick<CalendarExportOptions, "show_title" | "show_details">,
): MeasuredCalendar {
  const grid_width = CELL_WIDTH * 7;
  context.font = "700 36px system-ui, sans-serif";
  const title_lines = options.show_title
    ? wrap_text(context, title.trim() || "Calendar", grid_width)
    : [];
  const layouts = build_task_layouts(context, days);
  const row_count = Math.max(1, Math.ceil(days.length / 7));
  const row_heights = Array.from({ length: row_count }, (_, row) => {
    const tallest_day = Math.max(
      0,
      ...days.slice(row * 7, row * 7 + 7).map((day) =>
        day.tasks.reduce(
          (height, entry, index) => height + (layouts.get(entry)?.height ?? 48) + (index > 0 ? TASK_GAP : 0),
          0,
        )),
    );
    return Math.max(MIN_ROW_HEIGHT, DAY_HEADER_HEIGHT + 25 + tallest_day);
  });
  let grid_top = PAGE_PADDING;
  if (title_lines.length > 0) grid_top += title_lines.length * 44;
  if (options.show_details) grid_top += 36;
  if (title_lines.length > 0 || options.show_details) grid_top += 18;
  const logical_width = grid_width + PAGE_PADDING * 2;
  const logical_height = grid_top + WEEKDAY_HEIGHT + row_heights.reduce((sum, value) => sum + value, 0) + PAGE_PADDING;
  return { title_lines, layouts, row_heights, logical_width, logical_height, grid_top };
}

function normalized_dpi(value: number): number {
  if (!Number.isFinite(value)) return 96;
  return Math.min(300, Math.max(72, Math.round(value)));
}

function measure_for_export(
  days: CalendarDay[],
  title: string,
  options: CalendarExportOptions,
): MeasuredCalendar {
  const measurement_canvas = document.createElement("canvas");
  const context = measurement_canvas.getContext("2d");
  if (!context) throw new Error("Canvas rendering is unavailable");
  return measure_calendar(context, days, title, options);
}

function pixel_size(measured: MeasuredCalendar, dpi: number) {
  const scale = normalized_dpi(dpi) / LOGICAL_DPI;
  return {
    width: Math.ceil(measured.logical_width * scale),
    height: Math.ceil(measured.logical_height * scale),
  };
}

function exceeds_canvas_limit(width: number, height: number): boolean {
  return width > MAX_CANVAS_DIMENSION || height > MAX_CANVAS_DIMENSION || width * height > MAX_CANVAS_AREA;
}

export function estimate_calendar_export(
  periods: CalendarExportPeriod[],
  title: string,
  format: CalendarExportFormat,
  options: CalendarExportOptions,
): CalendarExportEstimate {
  const prepared = prepare_calendar_pages(periods, title, format, options);
  if (format === "pdf") return vector_pdf_estimate(prepared.length);
  const sizes = prepared.map((page) => pixel_size(measure_for_export(page.days, page.title, page.options), options.dpi));
  return summarize_export_sizes(sizes, prepared.length, format, options.png_layout);
}

export async function estimate_calendar_export_async(
  periods: CalendarExportPeriod[],
  title: string,
  format: CalendarExportFormat,
  options: CalendarExportOptions,
): Promise<CalendarExportEstimate> {
  const prepared = prepare_calendar_pages(periods, title, format, options);
  if (format === "pdf") return vector_pdf_estimate(prepared.length);
  const sizes: Array<{ width: number; height: number }> = [];
  for (const page of prepared) {
    await next_frame();
    sizes.push(pixel_size(measure_for_export(page.days, page.title, page.options), options.dpi));
  }
  return summarize_export_sizes(sizes, prepared.length, format, options.png_layout);
}

function vector_pdf_estimate(page_count: number): CalendarExportEstimate {
  return {
    width: 0,
    height: 0,
    megapixels: 0,
    memory_mb: 0,
    page_count,
    too_large: false,
  };
}

function summarize_export_sizes(
  sizes: Array<{ width: number; height: number }>,
  prepared_page_count: number,
  format: CalendarExportFormat,
  png_layout: CalendarPngLayout,
): CalendarExportEstimate {
  const largest = sizes.reduce(
    (result, size) => size.width * size.height > result.width * result.height ? size : result,
    { width: 0, height: 0 },
  );
  const separate_png = format === "png" && png_layout === "separate";
  const png_height = format === "png" && !separate_png
    ? sizes.reduce((sum, size) => sum + size.height, 0)
    : largest.height;
  const png_width = format === "png" && !separate_png
    ? Math.max(0, ...sizes.map((size) => size.width))
    : largest.width;
  return {
    width: png_width,
    height: png_height,
    megapixels: (png_width * png_height) / 1_000_000,
    memory_mb: (png_width * png_height * 4) / 1_048_576,
    page_count: format === "pdf" || separate_png ? prepared_page_count : 1,
    too_large: format === "png"
      ? separate_png
        ? sizes.some((size) => exceeds_canvas_limit(size.width, size.height))
        : exceeds_canvas_limit(png_width, png_height)
      : sizes.some((size) => exceeds_canvas_limit(size.width, size.height)),
  };
}

export interface PreparedCalendarPage {
  title: string;
  days: CalendarDay[];
  options: CalendarExportOptions;
}

export function prepare_calendar_pages(
  periods: CalendarExportPeriod[],
  title: string,
  format: CalendarExportFormat,
  options: CalendarExportOptions,
): PreparedCalendarPage[] {
  return periods.flatMap((period) => {
    const days = period.days;
    const page_title = periods.length > 1
      ? [options.show_title ? title.trim() : "", period.label].filter(Boolean).join("\n")
      : title;
    const page_options = periods.length > 1
      ? { ...options, show_title: true }
      : options;
    if (format === "pdf" && options.pdf_pagination === "weeks" && days.length > 7) {
      return Array.from({ length: Math.ceil(days.length / 7) }, (_, index) => ({
        title: page_title,
        days: days.slice(index * 7, index * 7 + 7),
        options: page_options,
      }));
    }
    return [{ title: page_title, days, options: page_options }];
  });
}

function localized_weekdays(): string[] {
  const sunday = new Date(2024, 0, 7);
  return Array.from({ length: 7 }, (_, index) => {
    const date = new Date(sunday);
    date.setDate(sunday.getDate() + index);
    return date.toLocaleDateString(EXPORT_LOCALE, { weekday: "short" });
  });
}

function next_frame(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => resolve());
    else setTimeout(resolve, 0);
  });
}

async function render_calendar_canvas(
  days: CalendarDay[],
  title: string,
  options: CalendarExportOptions,
): Promise<CalendarCanvasResult> {
  const dpi = normalized_dpi(options.dpi);
  const scale = dpi / LOGICAL_DPI;
  const measured = measure_for_export(days, title, options);
  const { width: pixel_width, height: pixel_height } = pixel_size(measured, dpi);
  if (exceeds_canvas_limit(pixel_width, pixel_height)) {
    throw new Error("This calendar is too large at the selected DPI. Lower the DPI or export fewer months.");
  }

  await next_frame();
  const canvas = document.createElement("canvas");
  canvas.width = pixel_width;
  canvas.height = pixel_height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Canvas rendering is unavailable");
  context.scale(scale, scale);
  const palette = PALETTES[options.theme];
  const weekdays = localized_weekdays();

  context.fillStyle = palette.page;
  context.fillRect(0, 0, measured.logical_width, measured.logical_height);
  if (measured.title_lines.length > 0) {
    context.fillStyle = palette.text;
    context.font = "700 36px system-ui, sans-serif";
    measured.title_lines.forEach((line, index) =>
      context.fillText(line, PAGE_PADDING, PAGE_PADDING + 34 + index * 44));
  }
  if (options.show_details) {
    const entry_count = days.reduce((count, day) => count + day.tasks.length, 0);
    context.fillStyle = palette.secondary_text;
    context.font = "16px system-ui, sans-serif";
    context.fillText(
      `${entry_count} ${entry_count === 1 ? "entry" : "entries"} · ${new Date().toLocaleString(EXPORT_LOCALE)}`,
      PAGE_PADDING,
      PAGE_PADDING + measured.title_lines.length * 44 + 24,
    );
  }

  const grid_width = CELL_WIDTH * 7;
  context.fillStyle = palette.weekday;
  context.fillRect(PAGE_PADDING, measured.grid_top, grid_width, WEEKDAY_HEIGHT);
  context.fillStyle = palette.weekday_text;
  context.font = "700 13px system-ui, sans-serif";
  context.textAlign = "center";
  for (let index = 0; index < 7; index++) {
    context.fillText(weekdays[index], PAGE_PADDING + index * CELL_WIDTH + CELL_WIDTH / 2, measured.grid_top + 28);
  }
  context.textAlign = "left";

  let row_top = measured.grid_top + WEEKDAY_HEIGHT;
  for (let row = 0; row < measured.row_heights.length; row++) {
    for (let column = 0; column < 7; column++) {
      const day = days[row * 7 + column];
      if (!day) continue;
      const x = PAGE_PADDING + column * CELL_WIDTH;
      context.fillStyle = day.in_current_month ? palette.day : palette.outside_day;
      context.fillRect(x, row_top, CELL_WIDTH, measured.row_heights[row]);
      context.strokeStyle = palette.border;
      context.lineWidth = 1;
      context.strokeRect(x + 0.5, row_top + 0.5, CELL_WIDTH - 1, measured.row_heights[row] - 1);

      if (day.is_today) {
        context.fillStyle = palette.today;
        rounded_rect(context, x + 12, row_top + 10, 34, 34, 17);
        context.fill();
        context.fillStyle = palette.today_text;
      } else {
        context.fillStyle = day.in_current_month ? palette.text : palette.outside_text;
      }
      context.font = "700 16px system-ui, sans-serif";
      context.textAlign = "center";
      context.fillText(String(day.date.getDate()), x + 29, row_top + 33);
      context.textAlign = "left";
      if (day.tasks.length > 0) {
        context.fillStyle = palette.secondary_text;
        context.font = "12px system-ui, sans-serif";
        context.textAlign = "right";
        context.fillText(String(day.tasks.length), x + CELL_WIDTH - 13, row_top + 31);
        context.textAlign = "left";
      }

      let task_top = row_top + DAY_HEADER_HEIGHT;
      for (const entry of day.tasks) {
        const layout = measured.layouts.get(entry);
        if (!layout) continue;
        const card_x = x + 10;
        const card_width = CELL_WIDTH - 20;
        context.save();
        context.fillStyle = entry.preview ? palette.preview_card : entry.archived ? palette.subdued_card : palette.card;
        context.strokeStyle = entry.archived || entry.preview ? palette.secondary_text : palette.border;
        context.lineWidth = entry.archived || entry.preview ? 1 : 2;
        context.setLineDash(entry.preview ? [7, 5] : entry.archived ? [2, 4] : []);
        rounded_rect(context, card_x, task_top, card_width, layout.height, 7);
        context.fill();
        context.stroke();
        context.setLineDash([]);
        context.fillStyle = entry.archived || entry.preview
          ? "#94a3b8"
          : calendar_export_accent(entry.task.color, options.theme);
        rounded_rect(context, card_x + 6, task_top + 7, 4, layout.height - 14, 2);
        context.fill();

        const text_x = card_x + 17;
        context.fillStyle = entry.archived ? palette.secondary_text : palette.text;
        context.font = `${entry.archived || entry.preview ? "500" : "600"} 14px system-ui, sans-serif`;
        layout.title_lines.forEach((line, index) =>
          context.fillText(line, text_x, task_top + 18 + index * 18));
        const status = entry.archived ? " · Archived" : entry.preview ? " · Recurring preview" : "";
        context.fillStyle = palette.secondary_text;
        context.font = "12px system-ui, sans-serif";
        context.fillText(
          fit_text(context, `${entry.column.name}${status}`, card_width - 27),
          text_x,
          task_top + layout.title_lines.length * 18 + 22,
        );
        context.restore();
        task_top += layout.height + TASK_GAP;
      }
    }
    row_top += measured.row_heights[row];
    await next_frame();
  }
  return { canvas, pixel_width, pixel_height, dpi };
}

function canvas_blob(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob> {
  return new Promise<Blob>((resolve, reject) => {
    canvas.toBlob(
      (blob) => blob ? resolve(blob) : reject(new Error("Couldn't create the exported file")),
      type,
      quality,
    );
  });
}

function write_u32(buffer: Uint8Array, offset: number, value: number) {
  new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength).setUint32(offset, value, false);
}

function read_u32(buffer: Uint8Array, offset: number): number {
  return new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength).getUint32(offset, false);
}

function crc32(data: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function concat_bytes(parts: Uint8Array[]): Uint8Array {
  const result = new Uint8Array(parts.reduce((length, part) => length + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

export function add_png_dpi_metadata(png: Uint8Array, dpi: number): Uint8Array {
  const signature = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
  if (png.length < 33 || !signature.every((byte, index) => png[index] === byte)) {
    throw new Error("Invalid PNG data");
  }
  const chunks: Uint8Array[] = [];
  let offset = 8;
  let inserted = false;
  while (offset + 12 <= png.length) {
    const length = read_u32(png, offset);
    const end = offset + 12 + length;
    if (end > png.length) throw new Error("Invalid PNG chunk data");
    const type = new TextDecoder().decode(png.slice(offset + 4, offset + 8));
    if (type !== "pHYs") chunks.push(png.slice(offset, end));
    if (type === "IHDR" && !inserted) {
      const pixels_per_meter = Math.round(normalized_dpi(dpi) / 0.0254);
      const physical = new Uint8Array(21);
      write_u32(physical, 0, 9);
      physical.set([112, 72, 89, 115], 4);
      write_u32(physical, 8, pixels_per_meter);
      write_u32(physical, 12, pixels_per_meter);
      physical[16] = 1;
      write_u32(physical, 17, crc32(physical.slice(4, 17)));
      chunks.push(physical);
      inserted = true;
    }
    offset = end;
  }
  if (!inserted || offset !== png.length) throw new Error("Invalid PNG structure");
  return concat_bytes([signature, ...chunks]);
}

function pdf_color(value: string): Color {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value);
  if (hex) {
    const expanded = hex[1].length === 3
      ? hex[1].split("").map((part) => `${part}${part}`).join("")
      : hex[1];
    if (!pdf_rgb) throw new Error("PDF color support is not initialized");
    return pdf_rgb(
      Number.parseInt(expanded.slice(0, 2), 16) / 255,
      Number.parseInt(expanded.slice(2, 4), 16) / 255,
      Number.parseInt(expanded.slice(4, 6), 16) / 255,
    );
  }
  const channels = /^rgb\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)$/i.exec(value);
  if (!pdf_rgb) throw new Error("PDF color support is not initialized");
  if (channels) return pdf_rgb(Number(channels[1]) / 255, Number(channels[2]) / 255, Number(channels[3]) / 255);
  return pdf_rgb(0.39, 0.45, 0.55);
}

interface CalendarPdfFonts {
  fallbacks: PDFFont[];
  outline?: CalendarPdfOutlineFont;
}

type FontkitPathCommand =
  | { command: "moveTo" | "lineTo"; args: [number, number] }
  | { command: "quadraticCurveTo"; args: [number, number, number, number] }
  | { command: "bezierCurveTo"; args: [number, number, number, number, number, number] }
  | { command: "closePath"; args: [] };

interface CalendarPdfOutlineFont {
  unitsPerEm: number;
  hasGlyphForCodePoint(code_point: number): boolean;
  layout(value: string): {
    glyphs: Array<{ path: { commands: FontkitPathCommand[] } }>;
    positions: Array<{
      xAdvance: number;
      yAdvance: number;
      xOffset: number;
      yOffset: number;
    }>;
    advanceWidth: number;
  };
}

function pdf_standard_font_for_character(fonts: CalendarPdfFonts, character: string): PDFFont | undefined {
  for (const font of fonts.fallbacks) {
    try {
      font.encodeText(character);
      return font;
    } catch {
      // Try the next non-embedded standard font.
    }
  }
  return undefined;
}

function outline_font_supports(font: CalendarPdfOutlineFont | undefined, character: string): boolean {
  const code_point = character.codePointAt(0);
  return code_point !== undefined && Boolean(font?.hasGlyphForCodePoint(code_point));
}

function pdf_safe_text(fonts: CalendarPdfFonts, value: string): string {
  return Array.from(value).map((character) => {
    if (pdf_standard_font_for_character(fonts, character)) return character;
    return outline_font_supports(fonts.outline, character) ? character : "?";
  }).join("");
}

function pdf_text_width(fonts: CalendarPdfFonts, value: string, size: number): number {
  return Array.from(value).reduce((width, character) => {
    const standard = pdf_standard_font_for_character(fonts, character);
    if (standard) return width + standard.widthOfTextAtSize(character, size);
    if (outline_font_supports(fonts.outline, character)) {
      return width + fonts.outline!.layout(character).advanceWidth / fonts.outline!.unitsPerEm * size;
    }
    return width + fonts.fallbacks[0].widthOfTextAtSize("?", size);
  }, 0);
}

function svg_number(value: number): string {
  const rounded = Math.round(value * 100) / 100;
  return String(Object.is(rounded, -0) ? 0 : rounded);
}

export function fontkit_path_to_flipped_svg(commands: FontkitPathCommand[]): string {
  return commands.map((entry) => {
    const args = entry.args;
    switch (entry.command) {
      case "moveTo": return `M${svg_number(args[0]!)} ${svg_number(-args[1]!)}`;
      case "lineTo": return `L${svg_number(args[0]!)} ${svg_number(-args[1]!)}`;
      case "quadraticCurveTo":
        return `Q${svg_number(args[0]!)} ${svg_number(-args[1]!)} ${svg_number(args[2]!)} ${svg_number(-args[3]!)}`;
      case "bezierCurveTo":
        return `C${svg_number(args[0]!)} ${svg_number(-args[1]!)} ${svg_number(args[2]!)} ${svg_number(-args[3]!)} ${svg_number(args[4]!)} ${svg_number(-args[5]!)}`;
      case "closePath": return "Z";
    }
  }).join("");
}

function draw_outline_character(
  page: PDFPage,
  font: CalendarPdfOutlineFont,
  character: string,
  left: number,
  baseline: number,
  size: number,
  color: Color,
) {
  const run = font.layout(character);
  const glyph_scale = size / font.unitsPerEm;
  let pen_x = 0;
  let pen_y = 0;
  run.glyphs.forEach((glyph, index) => {
    const position = run.positions[index];
    if (!position) return;
    if (glyph.path.commands.length > 0) {
      page.drawSvgPath(fontkit_path_to_flipped_svg(glyph.path.commands), {
        x: left + (pen_x + position.xOffset) * glyph_scale,
        y: baseline + (pen_y + position.yOffset) * glyph_scale,
        scale: glyph_scale,
        color,
      });
    }
    pen_x += position.xAdvance;
    pen_y += position.yAdvance;
  });
}

function measure_pdf_calendar(
  fonts: CalendarPdfFonts,
  days: CalendarDay[],
  title: string,
  options: Pick<CalendarExportOptions, "show_title" | "show_details">,
): MeasuredCalendar {
  const grid_width = CELL_WIDTH * 7;
  const title_lines = options.show_title
    ? wrap_calendar_text(pdf_safe_text(fonts, title.trim() || "Calendar"), grid_width, (value) => pdf_text_width(fonts, value, 36))
    : [];
  const layouts = new Map<CalendarTask, TaskLayout>();
  for (const day of days) {
    for (const entry of day.tasks) {
      const safe_title = pdf_safe_text(fonts, task_title(entry));
      const title_lines = wrap_calendar_text(safe_title, CELL_WIDTH - 47, (value) => pdf_text_width(fonts, value, 14));
      layouts.set(entry, { entry, title_lines, height: title_lines.length * 18 + 30 });
    }
  }
  const row_count = Math.max(1, Math.ceil(days.length / 7));
  const row_heights = Array.from({ length: row_count }, (_, row) => {
    const tallest_day = Math.max(
      0,
      ...days.slice(row * 7, row * 7 + 7).map((day) => day.tasks.reduce(
        (height, entry, index) => height + (layouts.get(entry)?.height ?? 48) + (index > 0 ? TASK_GAP : 0),
        0,
      )),
    );
    return Math.max(MIN_ROW_HEIGHT, DAY_HEADER_HEIGHT + 25 + tallest_day);
  });
  let grid_top = PAGE_PADDING;
  if (title_lines.length > 0) grid_top += title_lines.length * 44;
  if (options.show_details) grid_top += 36;
  if (title_lines.length > 0 || options.show_details) grid_top += 18;
  const logical_width = grid_width + PAGE_PADDING * 2;
  const logical_height = grid_top + WEEKDAY_HEIGHT + row_heights.reduce((sum, value) => sum + value, 0) + PAGE_PADDING;
  return { title_lines, layouts, row_heights, logical_width, logical_height, grid_top };
}

function fit_pdf_text(fonts: CalendarPdfFonts, value: string, size: number, max_width: number): string {
  const safe = pdf_safe_text(fonts, value);
  if (pdf_text_width(fonts, safe, size) <= max_width) return safe;
  const characters = Array.from(safe);
  let low = 0;
  let high = characters.length;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    const candidate = `${characters.slice(0, middle).join("")}...`;
    if (pdf_text_width(fonts, candidate, size) <= max_width) low = middle;
    else high = middle - 1;
  }
  return `${characters.slice(0, low).join("")}...`;
}

function vector_page_geometry(measured: MeasuredCalendar, paper: CalendarPdfPaper) {
  const content_width = measured.logical_width * 0.75;
  const content_height = measured.logical_height * 0.75;
  const target = paper === "a4"
    ? { width: 841.89, height: 595.28, margin: PDF_MARGIN }
    : paper === "a3"
      ? { width: 1190.55, height: 841.89, margin: PDF_MARGIN }
      : { width: content_width, height: content_height, margin: 0 };
  const scale = Math.min(
    (target.width - target.margin * 2) / measured.logical_width,
    (target.height - target.margin * 2) / measured.logical_height,
  );
  return {
    ...target,
    scale,
    offset_x: (target.width - measured.logical_width * scale) / 2,
    offset_top: (target.height - measured.logical_height * scale) / 2,
  };
}

function draw_vector_calendar_page(
  page: PDFPage,
  fonts: CalendarPdfFonts,
  days: CalendarDay[],
  title: string,
  options: CalendarExportOptions,
  paper: CalendarPdfPaper,
) {
  const measured = measure_pdf_calendar(fonts, days, title, options);
  const geometry = vector_page_geometry(measured, paper);
  page.setSize(geometry.width, geometry.height);
  const palette = PALETTES[options.theme];
  const scale = geometry.scale;
  const x = (logical: number) => geometry.offset_x + logical * scale;
  const baseline_y = (logical: number) => geometry.height - geometry.offset_top - logical * scale;
  const rect_y = (top: number, height: number) => geometry.height - geometry.offset_top - (top + height) * scale;
  const draw_text = (value: string, left: number, baseline: number, size: number, color: string) => {
    const safe = pdf_safe_text(fonts, value);
    let run = "";
    let run_font: PDFFont | undefined;
    let logical_x = left;
    const flush = () => {
      if (!run || !run_font) return;
      page.drawText(run, {
        x: x(logical_x),
        y: baseline_y(baseline),
        size: size * scale,
        font: run_font,
        color: pdf_color(color),
      });
      logical_x += run_font.widthOfTextAtSize(run, size);
      run = "";
    };
    for (const character of Array.from(safe)) {
      const character_font = pdf_standard_font_for_character(fonts, character);
      if (!character_font && fonts.outline && outline_font_supports(fonts.outline, character)) {
        flush();
        draw_outline_character(
          page,
          fonts.outline,
          character,
          x(logical_x),
          baseline_y(baseline),
          size * scale,
          pdf_color(color),
        );
        logical_x += pdf_text_width(fonts, character, size);
        run_font = undefined;
        continue;
      }
      const selected_font = character_font ?? fonts.fallbacks[0];
      if (run_font && selected_font !== run_font) flush();
      run_font = selected_font;
      run += character_font ? character : "?";
    }
    flush();
  };

  page.drawRectangle({ x: 0, y: 0, width: geometry.width, height: geometry.height, color: pdf_color(palette.page) });
  measured.title_lines.forEach((line, index) => draw_text(line, PAGE_PADDING, PAGE_PADDING + 34 + index * 44, 36, palette.text));
  if (options.show_details) {
    const entry_count = days.reduce((count, day) => count + day.tasks.length, 0);
    draw_text(
      `${entry_count} ${entry_count === 1 ? "entry" : "entries"} - ${new Date().toLocaleString(EXPORT_LOCALE)}`,
      PAGE_PADDING,
      PAGE_PADDING + measured.title_lines.length * 44 + 24,
      16,
      palette.secondary_text,
    );
  }

  const grid_width = CELL_WIDTH * 7;
  page.drawRectangle({
    x: x(PAGE_PADDING),
    y: rect_y(measured.grid_top, WEEKDAY_HEIGHT),
    width: grid_width * scale,
    height: WEEKDAY_HEIGHT * scale,
    color: pdf_color(palette.weekday),
  });
  localized_weekdays().forEach((weekday, index) => {
    const safe = pdf_safe_text(fonts, weekday);
    const width = pdf_text_width(fonts, safe, 13);
    draw_text(safe, PAGE_PADDING + index * CELL_WIDTH + (CELL_WIDTH - width) / 2, measured.grid_top + 28, 13, palette.weekday_text);
  });

  let row_top = measured.grid_top + WEEKDAY_HEIGHT;
  for (let row = 0; row < measured.row_heights.length; row += 1) {
    const row_height = measured.row_heights[row];
    for (let column = 0; column < 7; column += 1) {
      const day = days[row * 7 + column];
      if (!day) continue;
      const cell_x = PAGE_PADDING + column * CELL_WIDTH;
      page.drawRectangle({
        x: x(cell_x),
        y: rect_y(row_top, row_height),
        width: CELL_WIDTH * scale,
        height: row_height * scale,
        color: pdf_color(day.in_current_month ? palette.day : palette.outside_day),
        borderColor: pdf_color(palette.border),
        borderWidth: Math.max(0.5, scale),
      });

      const day_number = String(day.date.getDate());
      const day_width = pdf_text_width(fonts, day_number, 16);
      if (day.is_today) {
        page.drawCircle({
          x: x(cell_x + 29),
          y: baseline_y(row_top + 27),
          size: 17 * scale,
          color: pdf_color(palette.today),
        });
      }
      draw_text(
        day_number,
        cell_x + 29 - day_width / 2,
        row_top + 33,
        16,
        day.is_today ? palette.today_text : day.in_current_month ? palette.text : palette.outside_text,
      );
      if (day.tasks.length > 0) {
        const count = String(day.tasks.length);
        const count_width = pdf_text_width(fonts, count, 12);
        draw_text(count, cell_x + CELL_WIDTH - 13 - count_width, row_top + 31, 12, palette.secondary_text);
      }

      let task_top = row_top + DAY_HEADER_HEIGHT;
      for (const entry of day.tasks) {
        const layout = measured.layouts.get(entry);
        if (!layout) continue;
        const card_x = cell_x + 10;
        const card_width = CELL_WIDTH - 20;
        page.drawRectangle({
          x: x(card_x),
          y: rect_y(task_top, layout.height),
          width: card_width * scale,
          height: layout.height * scale,
          color: pdf_color(entry.preview ? palette.preview_card : entry.archived ? palette.subdued_card : palette.card),
          borderColor: pdf_color(entry.archived || entry.preview ? palette.secondary_text : palette.border),
          borderWidth: (entry.archived || entry.preview ? 1 : 2) * scale,
          borderDashArray: entry.preview ? [7 * scale, 5 * scale] : entry.archived ? [2 * scale, 4 * scale] : undefined,
        });
        page.drawRectangle({
          x: x(card_x + 6),
          y: rect_y(task_top + 7, layout.height - 14),
          width: 4 * scale,
          height: (layout.height - 14) * scale,
          color: pdf_color(entry.archived || entry.preview ? "#94a3b8" : calendar_export_accent(entry.task.color, options.theme)),
        });

        const text_x = card_x + 17;
        layout.title_lines.forEach((line, index) =>
          draw_text(line, text_x, task_top + 18 + index * 18, 14, entry.archived ? palette.secondary_text : palette.text));
        const status = entry.archived ? " - Archived" : entry.preview ? " - Recurring preview" : "";
        draw_text(
          fit_pdf_text(fonts, `${entry.column.name}${status}`, 12, card_width - 27),
          text_x,
          task_top + layout.title_lines.length * 18 + 22,
          12,
          palette.secondary_text,
        );
        task_top += layout.height + TASK_GAP;
      }
    }
    row_top += row_height;
  }
}

export async function build_vector_calendar_pdf(
  pages: PreparedCalendarPage[],
  paper: CalendarPdfPaper,
  outline_font?: CalendarPdfOutlineFont,
): Promise<Uint8Array> {
  if (pages.length === 0) throw new Error("A PDF needs at least one page");
  const { PDFDocument, StandardFonts, rgb } = await import("pdf-lib");
  pdf_rgb = rgb;
  const document = await PDFDocument.create();
  // PDF has no CSS-like system font fallback. These standard fonts are supplied
  // by the viewer and add no font files to the exported document.
  const fonts: CalendarPdfFonts = {
    fallbacks: await Promise.all([
      StandardFonts.Helvetica,
      StandardFonts.TimesRoman,
      StandardFonts.Courier,
      StandardFonts.Symbol,
      StandardFonts.ZapfDingbats,
    ].map((font) => document.embedFont(font))),
    outline: outline_font,
  };
  document.setTitle("Calendar export");
  document.setCreator("Cardbe");
  for (const prepared of pages) {
    const page = document.addPage();
    draw_vector_calendar_page(page, fonts, prepared.days, prepared.title, prepared.options, paper);
    await next_frame();
  }
  return document.save({ useObjectStreams: false });
}

async function create_calendar_outline_font(font_data: ArrayBuffer | Uint8Array): Promise<CalendarPdfOutlineFont> {
  // The package's ESM build exports a default object, but its declarations only list named members.
  const { default: fontkit } = await import("@pdf-lib/fontkit") as unknown as { default: FontkitModule };
  const bytes = font_data instanceof Uint8Array ? font_data : new Uint8Array(font_data);
  const loaded = fontkit.create(bytes) as FontkitFont | FontkitCollection;
  if (!("fonts" in loaded)) return loaded as unknown as CalendarPdfOutlineFont;
  const preferred = loaded.fonts.find((candidate) =>
    /(?:TC|Traditional|JhengHei|PingFang)/i.test(candidate.postscriptName ?? ""));
  const selected = preferred ?? loaded.fonts[0];
  if (!selected) throw new Error("The system font collection is empty");
  return selected as unknown as CalendarPdfOutlineFont;
}

let calendar_pdf_font: Promise<ArrayBuffer> | undefined;

function load_calendar_pdf_font(): Promise<ArrayBuffer> {
  calendar_pdf_font ??= invoke<ArrayBuffer>("get_calendar_pdf_font").catch((error) => {
    calendar_pdf_font = undefined;
    throw new Error(typeof error === "string" ? error : "Couldn't load a system font for PDF export");
  });
  return calendar_pdf_font;
}

function calendar_pdf_needs_outlines(pages: PreparedCalendarPage[]): boolean {
  return pages.some((page) =>
    /[^\x00-\x7f]/.test(page.title)
    || page.days.some((day) => day.tasks.some((entry) =>
      /[^\x00-\x7f]/.test(task_title(entry)) || /[^\x00-\x7f]/.test(entry.column.name))));
}

export async function render_calendar_export(
  periods: CalendarExportPeriod[],
  title: string,
  format: CalendarExportFormat,
  options: CalendarExportOptions,
): Promise<Blob> {
  const prepared = prepare_calendar_pages(periods, title, format, options);
  if (prepared.length === 0) throw new Error("Select at least one month to export");
  if (format === "pdf") {
    const outline_font = calendar_pdf_needs_outlines(prepared)
      ? await create_calendar_outline_font(await load_calendar_pdf_font())
      : undefined;
    const pdf = await build_vector_calendar_pdf(prepared, options.pdf_paper, outline_font);
    return new Blob([pdf], { type: "application/pdf" });
  }

  const estimate = estimate_calendar_export(periods, title, format, { ...options, png_layout: "combined" });
  if (estimate.too_large) {
    throw new Error("This multi-month image is too large. Lower the DPI or export fewer months.");
  }
  const output = document.createElement("canvas");
  output.width = estimate.width;
  output.height = estimate.height;
  const output_context = output.getContext("2d");
  if (!output_context) throw new Error("Canvas rendering is unavailable");
  output_context.fillStyle = PALETTES[options.theme].page;
  output_context.fillRect(0, 0, output.width, output.height);
  let y = 0;
  for (const page of prepared) {
    const rendered = await render_calendar_canvas(page.days, page.title, page.options);
    output_context.drawImage(rendered.canvas, 0, y);
    y += rendered.pixel_height;
    rendered.canvas.width = 1;
    rendered.canvas.height = 1;
    await next_frame();
  }
  const png = new Uint8Array(await (await canvas_blob(output, "image/png")).arrayBuffer());
  return new Blob([add_png_dpi_metadata(png, normalized_dpi(options.dpi))], { type: "image/png" });
}

function safe_filename_stem(filename: string): string {
  const without_extension = filename.trim().replace(/\.(png|pdf)$/i, "");
  return without_extension.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-").replace(/\s+/g, " ") || "calendar";
}

function safe_filename(filename: string, format: CalendarExportFormat): string {
  return `${safe_filename_stem(filename)}.${format}`;
}

async function available_png_path(directory: string, stem: string): Promise<string> {
  for (let copy = 1; copy <= 999; copy += 1) {
    const suffix = copy === 1 ? "" : ` (${copy})`;
    const file_path = await join(directory, `${stem}${suffix}.png`);
    if (!(await exists(file_path))) return file_path;
  }
  throw new Error("Couldn't find an available file name in the selected folder");
}

export async function save_split_calendar_png_export(
  periods: CalendarExportPeriod[],
  title: string,
  options: CalendarExportOptions,
  on_progress?: (completed: number, total: number) => void,
): Promise<number | null> {
  const directory = await open({
    directory: true,
    multiple: false,
    recursive: false,
    title: "Choose a folder for calendar images",
  });
  if (!directory) return null;

  const pages = prepare_calendar_pages(periods, title, "png", options);
  if (pages.length === 0) throw new Error("Select at least one month to export");
  const number_width = String(pages.length).length;
  on_progress?.(0, pages.length);

  for (let index = 0; index < pages.length; index += 1) {
    const page = pages[index];
    const rendered = await render_calendar_canvas(page.days, page.title, page.options);
    const png = new Uint8Array(await (await canvas_blob(rendered.canvas, "image/png")).arrayBuffer());
    rendered.canvas.width = 1;
    rendered.canvas.height = 1;
    const label = safe_filename_stem(periods[index]?.label ?? `Month ${index + 1}`);
    const order = String(index + 1).padStart(number_width, "0");
    const file_path = await available_png_path(directory, `${order} - ${label}`);
    await writeFile(file_path, add_png_dpi_metadata(png, normalized_dpi(options.dpi)), { createNew: true });
    on_progress?.(index + 1, pages.length);
    await next_frame();
  }
  return pages.length;
}

export async function save_calendar_export(
  blob: Blob,
  filename: string,
  format: CalendarExportFormat,
): Promise<boolean> {
  const file_path = await save({
    filters: [{ name: format === "pdf" ? "PDF document" : "PNG image", extensions: [format] }],
    defaultPath: safe_filename(filename, format),
  });
  if (!file_path) return false;
  await writeFile(file_path, new Uint8Array(await blob.arrayBuffer()));
  return true;
}
