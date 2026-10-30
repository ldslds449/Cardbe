import { expect, it } from "vite-plus/test";
import en from "../../../../messages/en.json";
import zh from "../../../../messages/zh-TW.json";

it("preserves Chinese daily plan messages without encoding replacements", () => {
  const keys = Object.keys(en).filter((key) => key.startsWith("daily_plan_"));
  expect(keys.length).toBeGreaterThan(0);
  expect(zh.daily_plan_title).toBe("\u4eca\u5929\u8981\u505a");
  // Plural messages are objects; check all of their text.
  const text = (value: unknown) =>
    typeof value === "string" ? value : JSON.stringify(value);
  for (const key of keys) {
    const chinese = text(zh[key as keyof typeof zh]);
    const english = text(en[key as keyof typeof en]);
    expect(chinese).toMatch(/\p{Script=Han}/u);
    expect(chinese).not.toMatch(/[?\uFFFD]/u);
    expect(english).not.toMatch(/\p{L}\?\p{L}|\?{2,}|\uFFFD/u);
  }
});
