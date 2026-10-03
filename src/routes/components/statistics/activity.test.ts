import { expect, it } from "vite-plus/test";
import { archive_activity } from "./activity";

it("groups archives by local day and handles leap years, streaks and future dates", () => {
  const result = archive_activity(
    [
      new Date(2023, 11, 31),
      new Date(2024, 1, 28, 23),
      new Date(2024, 1, 29, 0),
      new Date(2024, 1, 29, 12),
      new Date(2024, 2, 2),
      new Date(2024, 2, 4),
    ],
    2024,
    new Date(2024, 2, 3),
  );
  expect(result.total).toBe(4);
  expect(result.active_days).toBe(3);
  expect(result.longest_streak).toBe(2);
  expect(result.days.filter((day) => day.in_year)).toHaveLength(366);
  expect(result.days.length % 7).toBe(0);
  expect(result.days[0].date.getDay()).toBe(0);
  expect(
    result.days.find(
      (day) => day.date.getTime() === new Date(2024, 1, 29).getTime(),
    )?.count,
  ).toBe(2);
  expect(archive_activity([], 2025).total).toBe(0);
});
