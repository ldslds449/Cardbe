export type MarkdownHeading = {
  level: number;
  text: string;
  /** Offset of the first character of the heading block. */
  start: number;
  /** Offset just after the heading block, including its line break. */
  end: number;
};

export type MarkdownSection = {
  heading?: MarkdownHeading;
  start: number;
  end: number;
};

export type TextTarget = { offset: number; text: string };

const FENCE_PATTERN = /^ {0,3}(`{3,}|~{3,})/;
const ATX_PATTERN = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const SETEXT_PATTERN = /^ {0,3}(=+|-+)[ \t]*$/;
// Lines that cannot be the text of a setext heading.
const NON_PARAGRAPH_PATTERN = /^ {0,3}(?:[-*+>|]|\d+[.)]|#|`{3,}|~{3,})/;
// Reference definitions and footnotes resolve across the whole document, so
// such descriptions are never split into independently rendered sections.
const DEFINITION_PATTERN = /^ {0,3}\[[^\]\n]+\]:/m;

type Line = { text: string; start: number; end: number };

function split_lines(markdown: string): Line[] {
  const lines: Line[] = [];
  let start = 0;
  while (start <= markdown.length) {
    const newline = markdown.indexOf("\n", start);
    const end = newline === -1 ? markdown.length : newline + 1;
    lines.push({
      text: markdown.slice(start, newline === -1 ? end : newline),
      start,
      end,
    });
    if (newline === -1) {
      break;
    }
    start = end;
  }
  return lines;
}

/** Converts inline Markdown in a heading to its readable text. */
export function plain_heading_text(markdown: string): string {
  return markdown
    .replace(/\[\[([^\]\n|]+)\|task_\d+\]\]/g, "$1")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*_~]/g, "")
    .replace(/\\(.)/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

/** ATX and setext headings outside fenced code, in document order. */
export function markdown_outline(markdown: string): MarkdownHeading[] {
  const lines = split_lines(markdown.replace(/\r\n?/g, "\n"));
  const headings: MarkdownHeading[] = [];
  let fence: string | null = null;
  let paragraph_start: number | null = null;

  for (const [index, line] of lines.entries()) {
    const fence_match = FENCE_PATTERN.exec(line.text);
    if (fence) {
      if (
        fence_match &&
        fence_match[1][0] === fence[0] &&
        fence_match[1].length >= fence.length &&
        line.text.trim() === fence_match[1]
      ) {
        fence = null;
      }
      continue;
    }
    if (fence_match) {
      fence = fence_match[1];
      paragraph_start = null;
      continue;
    }

    const atx = ATX_PATTERN.exec(line.text);
    if (atx) {
      headings.push({
        level: atx[1].length,
        text: plain_heading_text(atx[2] ?? ""),
        start: line.start,
        end: line.end,
      });
      paragraph_start = null;
      continue;
    }

    const setext = SETEXT_PATTERN.exec(line.text);
    if (setext && paragraph_start !== null) {
      const text = lines
        .slice(paragraph_start, index)
        .map((paragraph_line) => paragraph_line.text)
        .join(" ");
      headings.push({
        level: setext[1][0] === "=" ? 1 : 2,
        text: plain_heading_text(text),
        start: lines[paragraph_start].start,
        end: line.end,
      });
      paragraph_start = null;
      continue;
    }

    if (line.text.trim() === "") {
      paragraph_start = null;
    } else if (
      paragraph_start === null &&
      !NON_PARAGRAPH_PATTERN.test(line.text) &&
      !/^ {4}/.test(line.text)
    ) {
      paragraph_start = index;
    } else if (
      paragraph_start !== null &&
      NON_PARAGRAPH_PATTERN.test(line.text)
    ) {
      paragraph_start = null;
    }
  }

  return headings.filter((heading) => heading.text.length > 0);
}

/**
 * Splits Markdown at its highest-level headings. The first section holds any
 * content before the first such heading. Returns one section when splitting
 * could change how the document renders.
 */
export function split_markdown_sections(
  markdown: string,
  headings = markdown_outline(markdown),
): MarkdownSection[] {
  const whole = [{ start: 0, end: markdown.length }];
  if (
    headings.length === 0 ||
    markdown.includes("\r") ||
    DEFINITION_PATTERN.test(markdown)
  ) {
    return whole;
  }

  const top_level = Math.min(...headings.map((heading) => heading.level));
  const section_headings = headings.filter(
    (heading) => heading.level === top_level,
  );
  const sections: MarkdownSection[] = [];
  if (markdown.slice(0, section_headings[0].start).trim()) {
    sections.push({ start: 0, end: section_headings[0].start });
  }
  for (const [index, heading] of section_headings.entries()) {
    sections.push({
      heading,
      start: heading.start,
      end: section_headings[index + 1]?.start ?? markdown.length,
    });
  }
  return sections;
}

export function count_occurrences(
  source: string,
  text: string,
  end: number,
): number {
  let count = 0;
  for (
    let index = source.indexOf(text);
    index !== -1 && index < end;
    index = source.indexOf(text, index + 1)
  ) {
    count += 1;
  }
  return count;
}

/** Shorter prefixes of rendered text are tried when Markdown syntax differs. */
export function text_target_prefixes(text: string): string[] {
  const characters = Array.from(text);
  return [
    ...new Set(
      [40, 20, 8].map((length) => characters.slice(0, length).join("")),
    ),
  ].filter((prefix) => prefix.trim().length > 0);
}

/**
 * Finds rendered block text in Markdown source, starting at `from`.
 * Falls back to `from` when Markdown syntax hides the text.
 */
export function locate_rendered_text(
  source: string,
  text: string,
  from = 0,
): TextTarget {
  for (const prefix of text_target_prefixes(text)) {
    const offset = source.indexOf(prefix, from);
    if (offset !== -1) {
      return { offset, text: prefix };
    }
  }
  return { offset: from, text: "" };
}

export function markdown_stats(markdown: string): {
  characters: number;
  lines: number;
} {
  // Counted in one pass: this runs on every keystroke in long descriptions.
  let characters = 0;
  let breaks = 0;
  for (let index = 0; index < markdown.length; index += 1) {
    const code = markdown.charCodeAt(index);
    if (code === 0x0d || code === 0x0a) {
      breaks += 1;
      if (code === 0x0d && markdown.charCodeAt(index + 1) === 0x0a) {
        index += 1;
      }
      continue;
    }
    characters += 1;
    // A surrogate pair is one code point.
    if (
      code >= 0xd800 &&
      code <= 0xdbff &&
      (markdown.charCodeAt(index + 1) & 0xfc00) === 0xdc00
    ) {
      index += 1;
    }
  }
  return {
    characters,
    lines: markdown.length === 0 ? 0 : breaks + 1,
  };
}
