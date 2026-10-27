import { expect, it } from "vite-plus/test";
import {
  date_range_bounds,
  highlight_parts,
  is_overdue,
} from "./task-explorer";

it("sends local date bounds with an exclusive next-day end to the server", () => {
  expect(date_range_bounds("2026-09-01", "2026-09-30")).toEqual([
    new Date("2026-09-01T00:00:00").getTime(),
    new Date("2026-10-01T00:00:00").getTime(),
  ]);
  expect(date_range_bounds("", "")).toEqual([null, null]);
  expect(date_range_bounds("2026-09-01", "")[1]).toBeNull();
  expect(date_range_bounds("", "2026-09-30")[0]).toBeNull();
  expect(date_range_bounds("2026-09-30", "2026-09-30")).toEqual([
    new Date("2026-09-30T00:00:00").getTime(),
    new Date("2026-10-01T00:00:00").getTime(),
  ]);
});

it("marks past active tasks overdue, including tasks from other boards", () => {
  const now = new Date(2000);
  expect(is_overdue({ due_time: new Date(1000) }, now)).toBe(true);
  expect(is_overdue({ due_time: now }, now)).toBe(false);
  expect(is_overdue({ due_time: new Date(3000) }, now)).toBe(false);
  expect(is_overdue({}, now)).toBe(false);
  expect(is_overdue({ due_time: new Date(1000) }, now, new Date(1500))).toBe(
    false,
  );
});

it("keeps a date-only due date on time until its day ends", () => {
  const due_time = new Date(2026, 9, 14);
  expect(is_overdue({ due_time }, new Date(2026, 9, 14, 23, 59))).toBe(false);
  expect(is_overdue({ due_time }, new Date(2026, 9, 15))).toBe(true);
  expect(
    is_overdue(
      { due_time: new Date(2026, 9, 14, 13) },
      new Date(2026, 9, 14, 14),
    ),
  ).toBe(true);
});

it("marks every case-insensitive literal match in a title", () => {
  expect(highlight_parts("Alpha beta ALPHA", "alpha")).toEqual([
    { text: "Alpha", match: true },
    { text: " beta ", match: false },
    { text: "ALPHA", match: true },
  ]);
  expect(highlight_parts("臺灣 🚀 工作報告", " 🚀 工作 ")).toEqual([
    { text: "臺灣 ", match: false },
    { text: "🚀 工作", match: true },
    { text: "報告", match: false },
  ]);
  expect(highlight_parts("a+b (c)", "+b (")).toEqual([
    { text: "a", match: false },
    { text: "+b (", match: true },
    { text: "c)", match: false },
  ]);
  // Fuzzy-only matches and empty queries keep the title unmarked.
  expect(highlight_parts("Release", "rls")).toEqual([
    { text: "Release", match: false },
  ]);
  expect(highlight_parts("Release", "  ")).toEqual([
    { text: "Release", match: false },
  ]);
  expect(highlight_parts("", "a")).toEqual([{ text: "", match: false }]);
});
