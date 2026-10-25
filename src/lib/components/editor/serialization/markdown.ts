import type { Editor, JSONContent } from "@tiptap/core";

export function markdownInsertionContent(editor: Editor, markdown: string) {
  const blocks = editor.markdown!.parse(markdown).content ?? [];
  // A single paragraph is inline content when inserted at the cursor.
  return blocks.length === 1 && blocks[0].type === "paragraph"
    ? (blocks[0].content ?? [])
    : blocks;
}

// Text that starts a line with block syntax (`1. `, `# `, `- `, setext
// underlines) would become a list or heading when parsed again. The
// serializer only escapes inline delimiters (and encodes `>`), so mark these
// positions and turn the marks into backslashes after serialization; a
// literal backslash in the text would itself be escaped.
const ESCAPE_MARK = "\uE000";
// An ordered list marker is escaped at its delimiter: `1\.`.
const ORDERED_MARKER = /^( {0,3}\d{1,9})(?=[.)](?:[ \t]|$))/;
const BLOCK_MARKER =
  /^( {0,3})(?=#{1,6}(?:[ \t]|$)|[-+](?:[ \t]|$)|=+[ \t]*$|-+[ \t]*$)/;

function escapeLineStarts(text: string, atLineStart: boolean): string {
  return text
    .split("\n")
    .map((line, index) =>
      index === 0 && !atLineStart
        ? line
        : line
            .replace(ORDERED_MARKER, `$1${ESCAPE_MARK}`)
            .replace(BLOCK_MARKER, `$1${ESCAPE_MARK}`),
    )
    .join("\n");
}

function markParagraphs(
  node: JSONContent,
  editor: Editor,
  formulas: { marker: string; values: string[] },
): JSONContent {
  if (node.type === "inlineMath") {
    const placeholder = `${formulas.marker}${formulas.values.length}\uE002`;
    formulas.values.push(editor.markdown!.serialize(node));
    return { type: "text", text: placeholder, marks: node.marks };
  }
  if (node.type === "codeBlock") {
    return node;
  }
  if (
    node.type === "text" &&
    node.text &&
    !node.marks?.some((mark) => mark.type === "code")
  ) {
    return { ...node, text: node.text.replaceAll("$", `${ESCAPE_MARK}$`) };
  }
  if (node.type !== "paragraph" || !node.content) {
    return node.content
      ? {
          ...node,
          content: node.content.map((child) =>
            markParagraphs(child, editor, formulas),
          ),
        }
      : node;
  }
  let atLineStart = true;
  return {
    ...node,
    content: node.content.map((child) => {
      const start = atLineStart;
      if (child.type === "hardBreak") {
        atLineStart = true;
        return child;
      }
      if (child.type !== "text" || !child.text) {
        atLineStart = false;
        return markParagraphs(child, editor, formulas);
      }
      atLineStart = child.text.endsWith("\n");
      // Marked text begins with its own delimiter (`**`, `[`, `` ` ``).
      return child.marks?.length
        ? markParagraphs(child, editor, formulas)
        : {
            ...child,
            text: escapeLineStarts(
              markParagraphs(child, editor, formulas).text!,
              start,
            ),
          };
    }),
  };
}

export function serializeMarkdown(editor: Editor): string {
  // shortcut: Tiptap 3.32 skips atom mark spans; serialize formulas as marked text until fixed upstream.
  const doc = editor.getJSON();
  const source = JSON.stringify(doc);
  let marker = "\uE001";
  while (source.includes(marker)) {
    marker += "\uE001";
  }
  const formulas = { marker, values: [] as string[] };
  return editor
    .markdown!.serialize(markParagraphs(doc, editor, formulas))
    .replaceAll(ESCAPE_MARK, "\\")
    .replace(
      new RegExp(`${marker}(\\d+)\uE002`, "g"),
      (_, index: string) => formulas.values[Number(index)],
    );
}
