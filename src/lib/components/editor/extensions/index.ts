import { EditorImageExtension } from "./image";
import Link from "@tiptap/extension-link";
import Placeholder from "@tiptap/extension-placeholder";
import {
  Table,
  TableCell,
  TableHeader,
  TableRow,
} from "@tiptap/extension-table";
import { isInTable, selectedRect } from "@tiptap/pm/tables";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import { Markdown } from "@tiptap/markdown";
import { Extension, InputRule, Mark } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { CodeHighlight } from "./code-highlight";
import { CodeScroll } from "./code-scroll";
import { MathInline, MathBlock, withMathMarks } from "./math";
import { CardReference, type CardReferenceSuggestion } from "./card-reference";
import {
  FootnoteDefinition,
  FootnoteReference,
  RawHtmlBlock,
  RawHtmlInline,
} from "./raw-markdown";
import CheckIcon from "@lucide/svelte/icons/check";
import { mount, unmount } from "svelte";
import { m } from "#lib/paraglide/messages.js";

const StyledTaskItem = TaskItem.extend({
  addOptions() {
    return {
      ...this.parent!(),
      // The default label repeats the item text, so it changes on every
      // keystroke.
      a11y: { checkboxLabel: () => m.editor_task_item_checkbox() },
    };
  },
  addInputRules() {
    return [
      // Typing `- [ ] ` first turns `- ` into a bullet item; convert that item
      // instead of nesting a task list inside it.
      new InputRule({
        find: /^\s*\[([ xX])?\]\s$/,
        handler: ({ state, range, match, chain }) => {
          const $from = state.doc.resolve(range.from);
          if (
            $from.depth < 2 ||
            $from.node(-1).type.name !== "listItem" ||
            $from.node(-2).type.name !== "bulletList" ||
            $from.index(-1) !== 0
          ) {
            return null;
          }
          // Lifting must come first: ProseMirror checks its result against
          // every mapping step already in the transaction.
          chain()
            .liftListItem("listItem")
            .toggleTaskList()
            .command(({ tr }) => {
              tr.delete(tr.mapping.map(range.from), tr.mapping.map(range.to));
              return true;
            })
            .updateAttributes(this.name, {
              checked: match[1]?.toLowerCase() === "x",
            })
            .run();
        },
      }),
      ...this.parent!(),
    ];
  },
  addNodeView() {
    const createView = this.parent!()!;
    return (props) => {
      const view = createView(props);
      // Tiptap mirrors the label into a visually hidden span before the item
      // text. Windows IMEs count that text when restoring the caret, so text
      // there misplaces typed characters; the input keeps its aria-label.
      view.dom.querySelector("label > span")?.remove();
      const icon = mount(CheckIcon, {
        target: view.dom.querySelector("label")!,
        props: { class: "task-item-check" },
      });
      return {
        ...view,
        destroy: () => {
          void unmount(icon);
          view.destroy?.();
        },
      };
    };
  },
});

// GFM table cells hold one line of inline content; block content would be
// flattened into raw `<br>` HTML that the Markdown preview strips.
const InlineTableCell = TableCell.extend({ content: "paragraph" });
const InlineTableHeader = TableHeader.extend({ content: "paragraph" });

// Images are inline so they can sit in text. A paragraph holding only an image
// would otherwise be parsed into a bare image at block level, which an inline
// image cannot occupy; keep it inside its paragraph.
const IMAGE_PARAGRAPH =
  /^ {0,3}!\[(?:\\.|[^\]\\\n])*\]\((?:[^()\s]|\([^()\s]*\))*(?:\s+(?:"[^"\n]*"|'[^'\n]*'))?\)[ \t]*(?:\n[ \t]*(?:\n|$)|$)/;

const ImageParagraph = Extension.create({
  name: "imageParagraph",
  markdownTokenizer: {
    name: "imageParagraph",
    level: "block",
    // Only at the start of a block, never interrupting a paragraph.
    start: () => -1,
    tokenize(source, _tokens, helper) {
      const match = IMAGE_PARAGRAPH.exec(source);
      return match
        ? {
            type: "imageParagraph",
            raw: match[0],
            tokens: helper.inlineTokens(match[0].trim()),
          }
        : undefined;
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode(
      "paragraph",
      undefined,
      helpers.parseInline(token.tokens ?? []),
    ),
});

export type ColumnAlign = "left" | "center" | "right" | null;

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    columnAlign: {
      /** Aligns every cell in the selected columns, as GFM stores it. */
      setColumnAlign: (align: ColumnAlign) => ReturnType;
    };
  }
}

const AlignedTable = Table.extend({
  addCommands() {
    return {
      ...this.parent?.(),
      setColumnAlign:
        (align) =>
        ({ state, tr, dispatch }) => {
          if (!isInTable(state)) {
            return false;
          }
          if (dispatch) {
            const { map, table, tableStart, left, right } = selectedRect(state);
            for (const position of new Set(
              map.cellsInRect({ left, right, top: 0, bottom: map.height }),
            )) {
              const cell = table.nodeAt(position)!;
              tr.setNodeMarkup(tableStart + position, null, {
                ...cell.attrs,
                align,
              });
            }
          }
          return true;
        },
    };
  },
});

export function createEditorExtensions(
  placeholder = "Start writing…",
  cardReferenceSuggestion: CardReferenceSuggestion | null = null,
) {
  return [
    StarterKit.extend({
      addExtensions() {
        return this.parent!().map((extension) =>
          extension instanceof Mark ? withMathMarks(extension) : extension,
        );
      },
    }).configure({
      link: false,
    }),
    CodeHighlight,
    CodeScroll,
    MathInline,
    MathBlock,
    CardReference.configure({ suggestion: cardReferenceSuggestion }),
    EditorImageExtension.configure({ inline: true }),
    ImageParagraph,
    RawHtmlInline,
    RawHtmlBlock,
    FootnoteReference,
    FootnoteDefinition,
    AlignedTable,
    TableRow,
    InlineTableCell,
    InlineTableHeader,
    TaskList,
    StyledTaskItem.configure({
      nested: true,
      HTMLAttributes: { "data-type": "taskItem" },
    }),
    withMathMarks(Link).configure({
      autolink: true,
      linkOnPaste: true,
      markdownLinks: true,
      openOnClick: false,
    }),
    Placeholder.configure({ placeholder }),
    Markdown.configure({ markedOptions: { gfm: true } }),
  ];
}
