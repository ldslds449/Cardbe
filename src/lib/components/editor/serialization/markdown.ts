import type { Editor, JSONContent } from "@tiptap/core";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";

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

// shortcut: Tiptap 3.32 skips atom mark spans; serialize formulas as marked text until fixed upstream.
function markFormulas(
  node: JSONContent,
  editor: Editor,
  render: (marked: JSONContent) => string,
): string {
  const source = JSON.stringify(node);
  let marker = "\uE001";
  while (source.includes(marker)) {
    marker += "\uE001";
  }
  const formulas = { marker, values: [] as string[] };
  return render(markParagraphs(node, editor, formulas))
    .replaceAll(ESCAPE_MARK, "\\")
    .replace(
      new RegExp(`${marker}(\\d+)\uE002`, "g"),
      (_, index: string) => formulas.values[Number(index)],
    );
}

// Undocumented in the type declarations; `serialize` renders each top-level
// block through it and joins them with a blank line.
type BlockRenderer = {
  renderNodeToMarkdown?: (
    node: JSONContent,
    parent: JSONContent,
    index: number,
    level: number,
  ) => string;
};

type SerializedBlock = {
  previous: ProseMirrorNode | undefined;
  index: number;
  markdown: string;
};

// Nodes are immutable and unchanged blocks keep their identity across
// transactions, so typing re-serializes only the edited block.
const blockCaches = new WeakMap<
  Editor,
  WeakMap<ProseMirrorNode, SerializedBlock>
>();

// Mirrors MarkdownManager.isEmptyOutput, applied by `serialize`.
function isEmptyOutput(markdown: string) {
  return (
    markdown
      .replace(/&nbsp;/g, "")
      .replace(/\u00A0/g, "")
      .trim() === ""
  );
}

/** Serializes the whole document at once, without the block cache. */
export function serializeDocument(editor: Editor): string {
  return markFormulas(editor.getJSON(), editor, (doc) =>
    editor.markdown!.serialize(doc),
  );
}

export function serializeMarkdown(editor: Editor): string {
  const manager = editor.markdown! as unknown as BlockRenderer;
  const renderBlock = manager.renderNodeToMarkdown?.bind(manager);
  if (!renderBlock) {
    return serializeDocument(editor);
  }
  let cache = blockCaches.get(editor);
  if (!cache) {
    cache = new WeakMap();
    blockCaches.set(editor, cache);
  }
  const blocks: string[] = [];
  let previous: ProseMirrorNode | undefined;
  editor.state.doc.forEach((node, _offset, index) => {
    let block = cache.get(node);
    if (!block || block.previous !== previous || block.index !== index) {
      // Paragraph rendering depends on the previous sibling and list items
      // on their index; nothing else in the parent is read.
      const siblings: JSONContent[] = [];
      if (previous) {
        siblings[index - 1] = previous.toJSON();
      }
      block = {
        previous,
        index,
        markdown: markFormulas(node.toJSON(), editor, (marked) =>
          renderBlock(marked, { type: "doc", content: siblings }, index, 0),
        ),
      };
      cache.set(node, block);
    }
    blocks.push(block.markdown);
    previous = node;
  });
  const markdown = blocks.join("\n\n");
  return isEmptyOutput(markdown) ? "" : markdown;
}
