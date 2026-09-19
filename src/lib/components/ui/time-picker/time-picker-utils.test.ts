import { Time } from "@internationalized/date";
import { describe, expect, it } from "vitest";

import {
  convert12HourTo24Hour,
  display12HourValue,
  getArrowByType,
  getDateByType,
  getValid12Hour,
  getValidHour,
  getValidMinuteOrSecond,
  getValidMinuteForStep,
  isValid12Hour,
  isValidHour,
  isValidMinuteOrSecond,
  setDateByType,
} from "./time-picker-utils";

describe("time input validation", () => {
  it.each([
    ["00", true],
    ["23", true],
    ["24", false],
    ["7", false],
  ])("validates 24-hour input %s", (value, expected) => {
    expect(isValidHour(value)).toBe(expected);
  });

  it("validates 12-hour and minute boundaries", () => {
    expect(isValid12Hour("01")).toBe(true);
    expect(isValid12Hour("12")).toBe(true);
    expect(isValid12Hour("00")).toBe(false);
    expect(isValidMinuteOrSecond("59")).toBe(true);
    expect(isValidMinuteOrSecond("60")).toBe(false);
  });

  it("normalizes incomplete and out-of-range values", () => {
    expect(getValidHour("7")).toBe("07");
    expect(getValidHour("30")).toBe("23");
    expect(getValid12Hour("0")).toBe("01");
    expect(getValidMinuteOrSecond("99")).toBe("59");
    expect(getValidMinuteOrSecond("text")).toBe("00");
  });

  it("normalizes minutes to the configured interval", () => {
    expect(getValidMinuteForStep("12", 5)).toBe("10");
    expect(getValidMinuteForStep("13", 5)).toBe("15");
    expect(getValidMinuteForStep("59", 5)).toBe("55");
  });
});

describe("time picker conversions", () => {
  it("wraps values changed with arrow keys", () => {
    expect(getArrowByType("23", 1, "hours")).toBe("00");
    expect(getArrowByType("00", -1, "hours")).toBe("23");
    expect(getArrowByType("12", 1, "12hours")).toBe("01");
    expect(getArrowByType("00", -1, "minutes")).toBe("59");
    expect(getArrowByType("55", 1, "minutes", 5)).toBe("00");
    expect(getArrowByType("00", -1, "minutes", 5)).toBe("55");
  });

  it.each([
    [12, "AM", 0],
    [12, "PM", 12],
    [1, "AM", 1],
    [1, "PM", 13],
  ] as const)("converts %i %s to %i:00", (hour, period, expected) => {
    expect(convert12HourTo24Hour(hour, period)).toBe(expected);
  });

  it("formats all 24 hours as two-digit 12-hour values", () => {
    const expected = [
      "12", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11",
      "12", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11",
    ];

    expect(expected.map((_, hour) => display12HourValue(hour))).toEqual(expected);
  });

  it("reads and updates Time values without mutating the original", () => {
    const original = new Time(23, 58, 57);
    const updated = setDateByType(original, "05", "minutes");

    expect(getDateByType(original, "minutes")).toBe("58");
    expect(getDateByType(updated, "minutes")).toBe("05");
    expect(updated.hour).toBe(23);
  });

  it("updates minutes using the configured interval", () => {
    const updated = setDateByType(new Time(9, 0, 30), "13", "minutes", undefined, 5);

    expect(updated.minute).toBe(15);
    expect(updated.second).toBe(30);
  });
});
