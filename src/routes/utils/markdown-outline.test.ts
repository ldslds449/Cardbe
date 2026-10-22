import { describe, expect, it } from "vite-plus/test";

import {
  count_occurrences,
  locate_rendered_text,
  markdown_outline,
  markdown_stats,
  split_markdown_sections,
} from "./markdown-outline";

describe("markdown outline", () => {
  it("collects ATX and setext headings with readable text", () => {
    const markdown = [
      "# **Plan** ##",
      "",
      "Intro",
      "",
      "Setup steps",
      "---",
      "",
      "## See [[Fix login|task_42]] and [docs](https://example.com)",
    ].join("\n");

    expect(
      markdown_outline(markdown).map(({ level, text }) => ({ level, text })),
    ).toEqual([
      { level: 1, text: "Plan" },
      { level: 2, text: "Setup steps" },
      { level: 2, text: "See Fix login and docs" },
    ]);
  });

  it("ignores headings in fenced code, lists, quotes, and tables", () => {
    const markdown = [
      "```md",
      "# not a heading",
      "```",
      "- item",
      "---",
      "> quote",
      "| a |",
      "|---|",
      "    # indented code",
      "~~~",
      "## still code",
      "~~~~",
      "### Real",
    ].join("\n");

    expect(markdown_outline(markdown).map((heading) => heading.text)).toEqual([
      "Real",
    ]);
  });

  it("keeps Chinese and emoji heading text intact", () => {
    expect(markdown_outline("## 任務說明 🚀\n內容")[0]).toMatchObject({
      level: 2,
      text: "任務說明 🚀",
      start: 0,
      end: 11,
    });
  });
});

describe("markdown sections", () => {
  it("splits at the highest heading level and keeps the preamble", () => {
    const markdown = "Intro\n\n## A\nbody\n### A.1\n## B\nend";
    const sections = split_markdown_sections(markdown);

    expect(
      sections.map((section) => markdown.slice(section.start, section.end)),
    ).toEqual(["Intro\n\n", "## A\nbody\n### A.1\n", "## B\nend"]);
    expect(sections[1].heading?.text).toBe("A");
  });

  it("does not split documents with reference definitions", () => {
    const markdown = "# A\nSee [docs][1]\n# B\n\n[1]: https://example.com";
    expect(split_markdown_sections(markdown)).toEqual([
      { start: 0, end: markdown.length },
    ]);
  });

  it("returns the whole document without headings", () => {
    expect(split_markdown_sections("plain text")).toEqual([
      { start: 0, end: 10 },
    ]);
  });
});

describe("rendered text lookup", () => {
  it("finds rendered text after the section offset", () => {
    const source = "## A\nrepeat me\n## B\nrepeat me";
    expect(locate_rendered_text(source, "repeat me", 15)).toEqual({
      offset: 20,
      text: "repeat me",
    });
  });

  it("falls back to shorter prefixes and then the section start", () => {
    const source = "Some text before **bold** and after";
    expect(
      locate_rendered_text(
        source,
        "Some text before bold and after the rest of the line",
      ),
    ).toEqual({ offset: 0, text: "Some tex" });
    expect(locate_rendered_text(source, "missing", 5)).toEqual({
      offset: 5,
      text: "",
    });
  });

  it("counts earlier occurrences", () => {
    expect(count_occurrences("a b a b a", "a", 6)).toBe(2);
  });
});

describe("markdown stats", () => {
  it("counts Unicode characters without line breaks", () => {
    expect(markdown_stats("中文🚀\r\nab")).toEqual({ characters: 5, lines: 2 });
    expect(markdown_stats("")).toEqual({ characters: 0, lines: 0 });
  });
});
