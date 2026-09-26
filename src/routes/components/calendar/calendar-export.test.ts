import { describe, expect, it, vi } from "vitest";
import { create_column } from "../../type/column.svelte";
import { create_task } from "../../type/task.svelte";
import type { CalendarDay, CalendarTask } from "./calendar";
import {
  add_png_dpi_metadata,
  build_vector_calendar_pdf,
  calendar_export_accent,
  calendar_entry_key,
  fontkit_path_to_flipped_svg,
  prepare_calendar_pages,
  selected_calendar_days,
  wrap_calendar_text,
} from "./calendar-export";

function fixture() {
  const task = create_task("task_1", "Ship calendar export", "", "#3b82f6");
  task.due_time = new Date(2026, 8, 5, 9, 30);
  const column = create_column("column_1", "Doing", "", [task]);
  const entry: CalendarTask = { task, column, archived: false, preview: false };
  const day: CalendarDay = {
    date: new Date(2026, 8, 5),
    key: "2026-09-05",
    in_current_month: true,
    is_today: true,
    tasks: [entry],
  };
  return { day, entry };
}

describe("calendar image selection", () => {
  it("uses the occurrence and state in an entry key", () => {
    const { day, entry } = fixture();
    expect(calendar_entry_key(day, entry)).toBe(
      `2026-09-05:task_1:${entry.task.due_time?.getTime()}:active:actual`,
    );
    expect(calendar_entry_key(day, { ...entry, preview: true })).not.toBe(
      calendar_entry_key(day, entry),
    );
  });

  it("keeps the complete date grid while filtering entries", () => {
    const { day, entry } = fixture();
    const empty_day = {
      ...day,
      key: "2026-09-06",
      date: new Date(2026, 8, 6),
      tasks: [],
    };
    const selected = selected_calendar_days(
      [day, empty_day],
      [calendar_entry_key(day, entry)],
    );
    expect(selected).toHaveLength(2);
    expect(selected[0].tasks).toEqual([entry]);
    expect(selected[1].tasks).toEqual([]);
  });

  it("removes unselected entries", () => {
    const { day } = fixture();
    expect(selected_calendar_days([day], [])[0].tasks).toEqual([]);
  });
});

describe("calendar export formats", () => {
  it("builds PDF pages from vector text and graphics without raster page images", async () => {
    const { day, entry } = fixture();
    entry.task.title = "Vector calendar";
    const options = {
      theme: "light" as const,
      dpi: 150,
      show_title: true,
      show_details: true,
      pdf_paper: "a4" as const,
      pdf_pagination: "single" as const,
      png_layout: "combined" as const,
    };
    const pdf = await build_vector_calendar_pdf(
      [{ title: "September 2026", days: [day], options }],
      "a4",
    );
    const source = new TextDecoder().decode(pdf);
    expect(source.startsWith("%PDF-")).toBe(true);
    expect(source).toContain("/Type /Font");
    expect(source).not.toMatch(/\/FontFile(?:2|3)?\b/);
    expect(source).not.toContain("/Subtype /Image");
  });

  it("exports unsupported Unicode text without embedding a font file", async () => {
    const { day, entry } = fixture();
    entry.task.title = "\u4e2d\u6587\u4efb\u52d9";
    const layout = vi.fn((_value: string) => ({
      glyphs: [
        {
          path: {
            commands: [
              { command: "moveTo" as const, args: [0, 0] as [number, number] },
              {
                command: "lineTo" as const,
                args: [900, 0] as [number, number],
              },
              {
                command: "lineTo" as const,
                args: [900, 900] as [number, number],
              },
              { command: "closePath" as const, args: [] as [] },
            ],
          },
        },
      ],
      positions: [{ xAdvance: 1000, yAdvance: 0, xOffset: 0, yOffset: 0 }],
      advanceWidth: 1000,
    }));
    const outline_font: NonNullable<
      Parameters<typeof build_vector_calendar_pdf>[2]
    > = {
      unitsPerEm: 1000,
      hasGlyphForCodePoint: (code_point) => code_point > 0x7f,
      layout,
    };
    const options = {
      theme: "light" as const,
      dpi: 150,
      show_title: true,
      show_details: true,
      pdf_paper: "a4" as const,
      pdf_pagination: "single" as const,
      png_layout: "combined" as const,
    };

    const pdf = await build_vector_calendar_pdf(
      [{ title: "\u4e2d\u6587\u884c\u4e8b\u66c6", days: [day], options }],
      "a4",
      outline_font,
    );
    const source = new TextDecoder().decode(pdf);
    expect(source.startsWith("%PDF-")).toBe(true);
    expect(source).toContain("/BaseFont /Helvetica");
    expect(source).not.toMatch(/\/FontFile(?:2|3)?\b/);
    expect(layout).toHaveBeenCalledWith("\u4e2d");
  });

  it("converts font coordinates to a PDF-compatible SVG outline", () => {
    expect(
      fontkit_path_to_flipped_svg([
        { command: "moveTo", args: [0, 10] },
        { command: "lineTo", args: [10, 0] },
        { command: "closePath", args: [] },
      ]),
    ).toBe("M0 -10L10 0Z");
  });

  it("falls back through non-embedded PDF standard fonts", async () => {
    const { day, entry } = fixture();
    entry.task.title = "Delta \u0394 and Omega \u03a9";
    const options = {
      theme: "light" as const,
      dpi: 150,
      show_title: true,
      show_details: false,
      pdf_paper: "a4" as const,
      pdf_pagination: "single" as const,
      png_layout: "combined" as const,
    };

    const pdf = await build_vector_calendar_pdf(
      [{ title: "Symbol fallback", days: [day], options }],
      "a4",
    );
    const source = new TextDecoder().decode(pdf);
    expect(source).toContain("/BaseFont /Helvetica");
    expect(source).toContain("/BaseFont /Symbol");
    expect(source).not.toMatch(/\/FontFile(?:2|3)?\b/);
  });

  it("creates one PDF page per month or one page per week", () => {
    const { day } = fixture();
    const make_period = (label: string, month: number) => ({
      label,
      days: Array.from(
        { length: 42 },
        (_, index): CalendarDay => ({
          ...day,
          date: new Date(2026, month, index + 1),
          key: `${label}-${index}`,
          tasks: [],
        }),
      ),
    });
    const periods = [
      make_period("September 2026", 8),
      make_period("October 2026", 9),
    ];
    const options = {
      theme: "light" as const,
      dpi: 150,
      show_title: true,
      show_details: true,
      pdf_paper: "a4" as const,
      pdf_pagination: "single" as const,
      png_layout: "combined" as const,
    };

    const monthly = prepare_calendar_pages(periods, "Roadmap", "pdf", options);
    const weekly = prepare_calendar_pages(periods, "Roadmap", "pdf", {
      ...options,
      pdf_pagination: "weeks",
    });

    expect(monthly).toHaveLength(2);
    expect(monthly.map((page) => page.title)).toEqual([
      "Roadmap\nSeptember 2026",
      "Roadmap\nOctober 2026",
    ]);
    expect(weekly).toHaveLength(12);
    expect(weekly.every((page) => page.days.length === 7)).toBe(true);
  });

  it("wraps long Latin and CJK titles without exceeding the requested width", () => {
    const measure = (value: string) => Array.from(value).length;
    const latin = wrap_calendar_text(
      "A short word andaverylongword",
      8,
      measure,
    );
    const cjk = wrap_calendar_text(
      "這是一個需要自動換行的月曆標題",
      6,
      measure,
    );
    expect(latin.length).toBeGreaterThan(1);
    expect(cjk.length).toBeGreaterThan(1);
    expect([...latin, ...cjk].every((line) => measure(line) <= 8)).toBe(true);
  });

  it("raises low-contrast accents for a dark export", () => {
    expect(calendar_export_accent("#000000", "dark")).toBe(
      "rgb(128, 128, 128)",
    );
    expect(calendar_export_accent("#3b82f6", "dark")).toBe("#3b82f6");
  });

  it("adds physical DPI metadata to a PNG", () => {
    const png = new Uint8Array([
      137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1,
      0, 0, 0, 1, 8, 2, 0, 0, 0, 0, 0, 0, 0,
    ]);
    const result = add_png_dpi_metadata(png, 300);
    expect(new TextDecoder().decode(result.slice(37, 41))).toBe("pHYs");
    expect(new DataView(result.buffer).getUint32(41, false)).toBe(11_811);
    expect(result).toHaveLength(png.length + 21);
  });

  it("replaces an existing PNG physical-resolution chunk", () => {
    const png = new Uint8Array([
      137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1,
      0, 0, 0, 1, 8, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 112, 72, 89, 115, 0, 0,
      14, 196, 0, 0, 14, 196, 1, 0, 0, 0, 0,
    ]);
    const result = add_png_dpi_metadata(png, 150);
    const encoded = new TextDecoder().decode(result);
    expect(encoded.match(/pHYs/g)).toHaveLength(1);
    expect(new DataView(result.buffer).getUint32(41, false)).toBe(5_906);
    expect(result).toHaveLength(png.length);
  });
});
