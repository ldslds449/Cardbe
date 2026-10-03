import { expect, it } from "vite-plus/test";
import { date_range_bounds, is_overdue } from "./task-explorer";

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
