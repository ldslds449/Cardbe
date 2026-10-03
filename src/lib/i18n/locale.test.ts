import { describe, expect, it } from "vite-plus/test";
import { isLanguagePreference, resolveLocale } from "./locale";

describe("locale resolution", () => {
  it.each([
    ["zh-TW", "zh-TW"],
    ["zh-Hant", "zh-TW"],
    ["zh-Hant-TW", "zh-TW"],
    ["ZH_hant_TW", "zh-TW"],
    ["en", "en"],
    ["en-US", "en"],
    ["en-GB", "en"],
    ["ja-JP", "en"],
    ["zh-CN", "en"],
    [undefined, "en"],
  ] as const)("maps %s to %s", (system, expected) => {
    expect(resolveLocale("system", system)).toBe(expected);
  });
  it("gives explicit preferences priority", () => {
    expect(resolveLocale("zh-TW", "en-US")).toBe("zh-TW");
    expect(resolveLocale("en", "zh-TW")).toBe("en");
    expect(resolveLocale()).toBe("en");
  });
  it("validates persisted preferences", () => {
    expect(["en", "zh-TW", "system"].every(isLanguagePreference)).toBe(true);
    expect([undefined, "ja", {}, null].some(isLanguagePreference)).toBe(false);
  });
});
