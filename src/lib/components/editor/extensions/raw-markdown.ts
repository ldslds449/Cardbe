import { Node } from "@tiptap/core";

// Syntax the editor cannot edit but the Markdown preview understands (raw
// HTML, footnotes) is kept as uneditable atoms that serialize verbatim, so
// editing elsewhere in a description never rewrites it.

function rawAtomView(
  tag: "span" | "div" | "sup",
  className: string,
  text: (raw: string) => string = (raw) => raw,
) {
  return ({ node }: { node: { attrs: Record<string, unknown> } }) => {
    const dom = document.createElement(tag);
    dom.className = className;
    dom.textContent = text(String(node.attrs.raw ?? ""));
    dom.contentEditable = "false";
    // ProseMirror collapses whitespace in uneditable nodes.
    dom.style.whiteSpace = "pre-wrap";
    return { dom };
  };
}

const footnoteLabel = (raw: string) => /^\[\^([^\]]+)\]/.exec(raw)?.[1] ?? raw;

const rawAttribute = { raw: { default: "" } };

// Inline HTML tags (CommonMark "raw HTML"). Autolinks such as
// `<https://…>` do not match because a tag name cannot contain `:` or `@`.
const INLINE_HTML =
  /^(?:<!--[\s\S]*?-->|<\/[A-Za-z][A-Za-z0-9-]*\s*>|<[A-Za-z][A-Za-z0-9-]*(?:\s+[^\s"'>/=]+(?:\s*=\s*(?:[^\s"'=<>`]+|'[^']*'|"[^"]*"))?)*\s*\/?>)/;

export const RawHtmlInline = Node.create({
  name: "rawHtmlInline",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  addAttributes: () => rawAttribute,
  parseHTML: () => [{ tag: "span[data-raw-html-inline]" }],
  renderHTML: ({ node }) => [
    "span",
    { "data-raw-html-inline": "" },
    node.attrs.raw,
  ],
  addNodeView: () => rawAtomView("span", "markdown-raw markdown-raw-inline"),
  markdownTokenizer: {
    name: "rawHtmlInline",
    level: "inline",
    start: (source) => source.indexOf("<"),
    tokenize(source) {
      const match = INLINE_HTML.exec(source);
      return match
        ? { type: "rawHtmlInline", raw: match[0], text: match[0] }
        : undefined;
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode("rawHtmlInline", { raw: token.raw }),
  renderMarkdown: (node) => String(node.attrs?.raw ?? ""),
});

export const RawHtmlBlock = Node.create({
  name: "rawHtmlBlock",
  group: "block",
  atom: true,
  selectable: true,
  markdownTokenName: "html",
  addAttributes: () => rawAttribute,
  parseHTML: () => [{ tag: "div[data-raw-html-block]" }],
  renderHTML: ({ node }) => [
    "div",
    { "data-raw-html-block": "" },
    node.attrs.raw,
  ],
  addNodeView: () => rawAtomView("div", "markdown-raw markdown-raw-block"),
  // Inline HTML is tokenized by RawHtmlInline; only block tokens reach here.
  parseMarkdown: (token, helpers) =>
    token.block
      ? helpers.createNode("rawHtmlBlock", {
          raw: String(token.raw ?? "").replace(/\s+$/, ""),
        })
      : [],
  renderMarkdown: (node) => String(node.attrs?.raw ?? ""),
});

export const FootnoteReference = Node.create({
  name: "footnoteReference",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  addAttributes: () => rawAttribute,
  parseHTML: () => [{ tag: "sup[data-footnote-reference]" }],
  renderHTML: ({ node }) => [
    "sup",
    { "data-footnote-reference": "" },
    node.attrs.raw,
  ],
  addNodeView: () => rawAtomView("sup", "markdown-footnote-ref", footnoteLabel),
  markdownTokenizer: {
    name: "footnoteReference",
    level: "inline",
    start: (source) => source.indexOf("[^"),
    tokenize(source) {
      const match = /^\[\^[^\]\s]+\](?!:)/.exec(source);
      return match
        ? { type: "footnoteReference", raw: match[0], text: match[0] }
        : undefined;
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode("footnoteReference", { raw: token.raw }),
  renderMarkdown: (node) => String(node.attrs?.raw ?? ""),
});

// A definition continues on following non-blank lines and on indented
// paragraphs after blank lines, as in GitHub's footnote syntax.
const FOOTNOTE_DEFINITION =
  /^\[\^[^\]\s]+\]:[^\n]*(?:\n(?![ \t]*\n)(?!\[\^)[^\n]+)*(?:\n(?:[ \t]*\n)+(?: {2,}|\t)[^\n]*(?:\n(?![ \t]*\n)[^\n]+)*)*/;

export const FootnoteDefinition = Node.create({
  name: "footnoteDefinition",
  group: "block",
  atom: true,
  selectable: true,
  addAttributes: () => rawAttribute,
  parseHTML: () => [{ tag: "div[data-footnote-definition]" }],
  renderHTML: ({ node }) => [
    "div",
    { "data-footnote-definition": "" },
    node.attrs.raw,
  ],
  addNodeView: () =>
    rawAtomView(
      "div",
      "markdown-footnote",
      (raw) =>
        `${footnoteLabel(raw)}. ${raw.replace(/^\[\^[^\]]+\]:[ \t]*/, "")}`,
    ),
  markdownTokenizer: {
    name: "footnoteDefinition",
    level: "block",
    start: (source) => {
      const match = /(?:^|\n)\[\^[^\]\s]+\]:/.exec(source);
      return match ? match.index + (match[0].startsWith("\n") ? 1 : 0) : -1;
    },
    tokenize(source) {
      const match = FOOTNOTE_DEFINITION.exec(source);
      return match
        ? { type: "footnoteDefinition", raw: match[0], text: match[0] }
        : undefined;
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode("footnoteDefinition", { raw: token.raw }),
  renderMarkdown: (node) => String(node.attrs?.raw ?? ""),
});
