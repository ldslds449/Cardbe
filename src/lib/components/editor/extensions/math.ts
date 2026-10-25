import {
  InputRule,
  type NodeViewRenderer,
  type Mark,
  type JSONContent,
} from "@tiptap/core";
import { InlineMath, BlockMath } from "@tiptap/extension-mathematics";
import { mount, unmount } from "svelte";
import MathNodeView from "../math-node-view.svelte";
import { Plugin, PluginKey } from "@tiptap/pm/state";

export const mathPasteKey = new PluginKey("display-math-paste");

// shortcut: Tiptap 3.32 skips marks on inline atoms; remove when its parser preserves them.
export function withMathMarks(mark: Mark) {
  return mark.extend({
    parseMarkdown(token, helpers) {
      const parsed = this.parent?.(token, helpers);
      if (!parsed || Array.isArray(parsed) || !("mark" in parsed)) {
        return parsed ?? [];
      }
      const apply = (nodes: JSONContent[]): JSONContent[] =>
        nodes.map((node) =>
          node.type === "inlineMath"
            ? {
                ...node,
                marks: [
                  ...(node.marks ?? []),
                  {
                    type: parsed.mark,
                    ...(parsed.attrs ? { attrs: parsed.attrs } : {}),
                  },
                ],
              }
            : node.content
              ? { ...node, content: apply(node.content) }
              : node,
        );
      return { ...parsed, content: apply(parsed.content ?? []) };
    },
  });
}

const mathView: NodeViewRenderer = ({ node, editor, getPos }) => {
  const dom = document.createElement(node.isInline ? "span" : "div");
  const component = mount(MathNodeView, {
    target: dom,
    props: {
      source: node.attrs.latex,
      display: !node.isInline || !!node.attrs.display,
      editable: () => editor.isEditable,
      focus: () => {
        if (!editor.isDestroyed) {
          editor.commands.focus();
        }
      },
      save: (latex) => {
        const pos = getPos();
        if (
          pos !== undefined &&
          editor.isEditable &&
          !editor.isDestroyed &&
          editor.state.doc.nodeAt(pos)?.type === node.type
        ) {
          editor.view.dispatch(
            editor.state.tr.setNodeMarkup(pos, undefined, {
              ...node.attrs,
              latex,
            }),
          );
        }
      },
    },
  });
  return {
    dom,
    stopEvent: () => true,
    ignoreMutation: () => true,
    destroy: () => {
      void unmount(component);
    },
  };
};

export const MathInline = InlineMath.extend({
  addNodeView: () => mathView,
  addAttributes() {
    return {
      ...this.parent?.(),
      delimiter: { default: "$" },
      display: { default: false },
    };
  },
  parseMarkdown: (token) => ({
    type: "inlineMath",
    attrs: {
      latex: token.latex,
      delimiter: token.delimiter,
      display: String(token.delimiter).length > 1,
    },
  }),
  renderMarkdown: (node) => {
    const latex = String(node.attrs?.latex ?? "");
    // A longer fence keeps literal dollar runs inside the formula intact.
    const longest = (latex.match(/\$+/g) ?? []).reduce(
      (max, run) => Math.max(max, run.length),
      0,
    );
    const previous = String(node.attrs?.delimiter ?? "$");
    const delimiter =
      previous.length > longest ? previous : "$".repeat(longest + 1);
    const padding = latex.startsWith("$") || latex.endsWith("$") ? " " : "";
    return `${delimiter}${padding}${latex}${padding}${delimiter}`;
  },
  // Like remark-math, only a dollar run of the same length closes inline math.
  markdownTokenizer: {
    name: "inlineMath",
    level: "inline",
    start: (source) => source.indexOf("$"),
    tokenize(source) {
      const delimiter = /^\$+/.exec(source)?.[0];
      if (!delimiter) {
        return undefined;
      }
      for (const match of source.slice(delimiter.length).matchAll(/\$+/g)) {
        if (match[0].length === delimiter.length) {
          const end = delimiter.length + match.index;
          return {
            type: "inlineMath",
            raw: source.slice(0, end + delimiter.length),
            latex: source.slice(delimiter.length, end).trim(),
            delimiter,
          };
        }
      }
      return undefined;
    },
  },
  addInputRules() {
    return [
      new InputRule({
        find: /(?:^|[^$\\])(\$(?!\$)((?:\\[^\n]|[^$\\\n])+)\$)$/,
        handler: ({ state, range, match }) => {
          state.tr.replaceWith(
            range.to - match[1].length,
            range.to,
            this.type.create({ latex: match[2] }),
          );
        },
      }),
    ];
  },
});

export const MathBlock = BlockMath.extend({
  addNodeView: () => mathView,
  addProseMirrorPlugins() {
    const editor = this.editor;
    return [
      new Plugin({
        key: mathPasteKey,
        props: {
          handlePaste: (_view, event) => {
            const clipboard = event.clipboardData;
            if (
              !editor.isEditable ||
              editor.isActive("codeBlock") ||
              editor.isActive("code") ||
              !clipboard ||
              clipboard.getData("text/html")
            ) {
              return false;
            }
            const source = clipboard.getData("text/plain").trim();
            if (!source.startsWith("$$") || !source.endsWith("$$")) {
              return false;
            }
            const blocks = editor.markdown!.parse(source).content ?? [];
            return blocks.length === 1 && blocks[0].type === this.name
              ? editor.commands.insertContent(blocks)
              : false;
          },
        },
      }),
    ];
  },
  addKeyboardShortcuts() {
    return {
      Enter: () => {
        const { $from, empty } = this.editor.state.selection;
        if (
          !this.editor.isEditable ||
          !empty ||
          $from.parent.type.name !== "paragraph" ||
          !/^\${2,}$/.test($from.parent.textContent) ||
          $from.parentOffset !== $from.parent.content.size ||
          !$from
            .node(-1)
            .canReplaceWith($from.index(-1), $from.indexAfter(-1), this.type)
        ) {
          return false;
        }
        return this.editor.commands.insertContentAt(
          { from: $from.before(), to: $from.after() },
          { type: this.name, attrs: { latex: "" } },
        );
      },
    };
  },
  renderMarkdown: (node) => {
    const latex = String(node.attrs?.latex ?? "");
    const longest = (latex.match(/\$+/g) ?? []).reduce(
      (max, run) => Math.max(max, run.length),
      1,
    );
    const delimiter = "$".repeat(longest + 1);
    return `${delimiter}\n${latex}\n${delimiter}`;
  },
  markdownTokenizer: {
    name: "blockMath",
    level: "block",
    start: (source) => {
      const match = /(?:^|\n) {0,3}\${2,}(?!\$)/.exec(source);
      return match ? match.index + (match[0].startsWith("\n") ? 1 : 0) : -1;
    },
    tokenize(source) {
      const match =
        /^ {0,3}(\${2,})[^\S\n]*\n([\s\S]*?)\n {0,3}\1\$*[^\S\n]*(?:\n|$)/.exec(
          source,
        ) ?? /^ {0,3}(\${2,})(?!\$)([^\n]+?)\1\$*[^\S\n]*(?:\n|$)/.exec(source);
      return match
        ? { type: "blockMath", raw: match[0], latex: match[2] }
        : undefined;
    },
  },
  addInputRules() {
    return [
      new InputRule({
        find: /^(\${2,})(?!\$)([^$\n]+)\1$/,
        handler: ({ state, range, match }) => {
          const $from = state.doc.resolve(range.from);
          if (
            range.from !== $from.start() ||
            range.to !== $from.end() ||
            !$from
              .node(-1)
              .canReplaceWith($from.index(-1), $from.indexAfter(-1), this.type)
          ) {
            return null;
          }
          state.tr.replaceWith(
            $from.before(),
            $from.after(),
            this.type.create({ latex: match[2].trim() }),
          );
        },
      }),
    ];
  },
});
