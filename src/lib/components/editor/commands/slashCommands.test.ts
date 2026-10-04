import { overwriteGetLocale, getLocale } from "$lib/paraglide/runtime";
import { describe, expect, it } from "vite-plus/test";
import { filterSlashCommands, getSlashCommands } from "./slashCommands";

describe("slash commands", () => {
  it("registers the initial block commands", () => {
    expect(getSlashCommands().map((command) => command.id)).toEqual([
      "paragraph",
      "heading-1",
      "heading-2",
      "heading-3",
      "bullet-list",
      "ordered-list",
      "task-list",
      "blockquote",
      "code-block",
      "horizontal-rule",
    ]);
  });

  it("filters commands by labels and keywords", () => {
    expect(filterSlashCommands("h2").map((command) => command.id)).toEqual([
      "heading-2",
    ]);
    expect(filterSlashCommands("todo").map((command) => command.id)).toEqual([
      "task-list",
    ]);
  });
});

it("updates labels, descriptions and search on locale changes", () => {
  const original = getLocale;
  try {
    for (const locale of ["en", "zh-TW", "en"] as const) {
      overwriteGetLocale(() => locale);
      expect(getSlashCommands()[0].label).toBe(
        locale === "en" ? "Text" : "文字",
      );
      expect(getSlashCommands()[0].description).toBe(
        locale === "en" ? "Start with a plain paragraph" : "一般段落",
      );
      if (locale === "zh-TW") {
        expect(filterSlashCommands("標題")).toHaveLength(3);
        expect(filterSlashCommands("清單")).toHaveLength(3);
      }
    }
  } finally {
    overwriteGetLocale(original);
  }
});
