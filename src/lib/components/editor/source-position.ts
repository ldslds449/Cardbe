import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { count_occurrences } from "../../../routes/utils/markdown-outline";

// Text around a cursor is looked up on the other side, longest first;
// shorter probes are less likely to cross Markdown syntax.
const PROBE_LENGTHS = [40, 20, 8];

function head(text: string, length: number): string {
  return Array.from(text).slice(0, length).join("");
}

function tail(text: string, length: number): string {
  return Array.from(text).slice(-length).join("");
}

/** Finds the nth occurrence of text inside a single text block. */
export function textPosition(
  doc: ProseMirrorNode,
  text: string,
  occurrence: number,
): number | null {
  let seen = 0;
  let found: number | null = null;
  doc.descendants((node, position) => {
    if (found !== null) {
      return false;
    }
    if (!node.isTextblock) {
      return true;
    }
    const content = node.textContent;
    for (
      let index = content.indexOf(text);
      index !== -1;
      index = content.indexOf(text, index + 1)
    ) {
      if (seen === occurrence) {
        found = position + 1 + index;
        return false;
      }
      seen += 1;
    }
    return false;
  });
  return found;
}

// Counts occurrences of text inside text blocks that start before `end`.
function textOccurrences(
  doc: ProseMirrorNode,
  text: string,
  end: number,
): number {
  let count = 0;
  doc.descendants((node, position) => {
    if (position >= end) {
      return false;
    }
    if (!node.isTextblock) {
      return true;
    }
    const content = node.textContent;
    for (
      let index = content.indexOf(text);
      index !== -1 && position + 1 + index < end;
      index = content.indexOf(text, index + 1)
    ) {
      count += 1;
    }
    return false;
  });
  return count;
}

function nthIndex(source: string, text: string, occurrence: number): number {
  let index = source.indexOf(text);
  for (let seen = 0; index !== -1 && seen < occurrence; seen += 1) {
    index = source.indexOf(text, index + 1);
  }
  return index;
}

/** Maps a rich-text document position to an offset in its Markdown source. */
export function sourceOffset(
  doc: ProseMirrorNode,
  markdown: string,
  position: number,
): number {
  const $position = doc.resolve(position);
  const block = $position.parent;
  if (block.isTextblock) {
    const before = block.textBetween(0, $position.parentOffset);
    const after = block.textBetween($position.parentOffset, block.content.size);
    for (const length of PROBE_LENGTHS) {
      const forward = head(after, length);
      if (forward.trim()) {
        const index = nthIndex(
          markdown,
          forward,
          textOccurrences(doc, forward, position),
        );
        if (index !== -1) {
          return index;
        }
      }
      const backward = tail(before, length);
      if (backward.trim()) {
        const index = nthIndex(
          markdown,
          backward,
          textOccurrences(doc, backward, position - backward.length),
        );
        if (index !== -1) {
          return index + backward.length;
        }
      }
    }
  }
  const ratio = position / Math.max(1, doc.content.size);
  return Math.round(ratio * markdown.length);
}

/** Maps an offset in Markdown source to a rich-text document position. */
export function documentPosition(
  doc: ProseMirrorNode,
  markdown: string,
  offset: number,
): number {
  const lineStart = markdown.lastIndexOf("\n", offset - 1) + 1;
  const lineEnd = markdown.indexOf("\n", offset);
  const before = markdown.slice(lineStart, offset);
  const after = markdown.slice(offset, lineEnd === -1 ? undefined : lineEnd);
  for (const length of PROBE_LENGTHS) {
    const forward = head(after, length);
    if (forward.trim()) {
      const position = textPosition(
        doc,
        forward,
        count_occurrences(markdown, forward, offset),
      );
      if (position !== null) {
        return position;
      }
    }
    const backward = tail(before, length);
    if (backward.trim()) {
      const position = textPosition(
        doc,
        backward,
        count_occurrences(markdown, backward, offset - backward.length),
      );
      if (position !== null) {
        return position + backward.length;
      }
    }
  }
  const ratio = offset / Math.max(1, markdown.length);
  return Math.round(ratio * doc.content.size);
}
