import Image from "@tiptap/extension-image";
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
import { Extension } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { CodeHighlight } from "./code-highlight";
import { CodeScroll } from "./code-scroll";
import { CardReference } from "./card-reference";
import {
  FootnoteDefinition,
  FootnoteReference,
  RawHtmlBlock,
  RawHtmlInline,
} from "./raw-markdown";
import CheckIcon from "@lucide/svelte/icons/check";
import { mount, unmount } from "svelte";

const StyledTaskItem = TaskItem.extend({
  addNodeView() {
    const createView = this.parent!()!;
    return (props) => {
      const view = createView(props);
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

export function createEditorExtensions(placeholder = "Start writing…") {
  return [
    StarterKit.configure({
      link: false,
    }),
    CodeHighlight,
    CodeScroll,
    CardReference,
    Image.configure({ inline: true }),
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
    Link.configure({
      autolink: true,
      linkOnPaste: true,
      markdownLinks: true,
      openOnClick: false,
    }),
    Placeholder.configure({ placeholder }),
    Markdown.configure({ markedOptions: { gfm: true } }),
  ];
}
