import { Node } from "@tiptap/core";
import { PluginKey } from "@tiptap/pm/state";
import Suggestion, { type SuggestionOptions } from "@tiptap/suggestion";
import { mount, unmount } from "svelte";
import Link2Icon from "@lucide/svelte/icons/link-2";
import {
  create_card_reference,
  task_id_from_card_reference,
} from "../../../../routes/utils/card-reference";

export type CardReferenceSuggestion = Omit<SuggestionOptions, "editor">;

export const CardReferenceSuggestionKey = new PluginKey(
  "cardReferenceSuggestion",
);

export const CardReference = Node.create<{
  /** Opens the `[[` card picker at the cursor while typing. */
  suggestion: CardReferenceSuggestion | null;
}>({
  name: "cardReference",
  group: "inline",
  inline: true,
  atom: true,
  addOptions() {
    return { suggestion: null };
  },
  addProseMirrorPlugins() {
    const suggestion = this.options.suggestion;
    if (!suggestion) {
      return [];
    }
    return [
      Suggestion({
        pluginKey: CardReferenceSuggestionKey,
        char: "[[",
        // Card titles contain spaces, and `[[` may follow any character.
        allowSpaces: true,
        allowedPrefixes: null,
        ...suggestion,
        editor: this.editor,
        allow: (props) => {
          const { state, range } = props;
          const code = state.schema.marks.code;
          return (
            !state.doc.resolve(range.from).parent.type.spec.code &&
            !(code && state.doc.rangeHasMark(range.from, range.to, code)) &&
            // A closing bracket means a reference typed out by hand.
            !state.doc.textBetween(range.from, range.to).includes("]") &&
            (suggestion.allow?.(props) ?? true)
          );
        },
      }),
    ];
  },
  addAttributes() {
    return {
      title: { default: "Untitled card" },
      taskId: { default: null },
    };
  },
  parseHTML() {
    return [
      {
        tag: "a[data-card-reference]",
        getAttrs: (element) => {
          const taskId = task_id_from_card_reference(
            element.getAttribute("href"),
          );
          return taskId ? { taskId, title: element.textContent } : false;
        },
      },
    ];
  },
  renderHTML({ node }) {
    return [
      "a",
      {
        href: `#card-${node.attrs.taskId}`,
        "data-card-reference": "",
      },
      node.attrs.title,
    ];
  },
  markdownTokenizer: {
    name: "cardReference",
    level: "inline",
    start: (source) => source.indexOf("[["),
    tokenize(source) {
      const match = /^\[\[([^\n|]+)\|(task_\d+)\]\]/.exec(source);
      if (!match) {
        return undefined;
      }
      return {
        type: "cardReference",
        raw: match[0],
        title: match[1],
        taskId: match[2],
      };
    },
  },
  parseMarkdown: (token, helpers) =>
    helpers.createNode("cardReference", {
      title: token.title,
      taskId: token.taskId,
    }),
  renderMarkdown: (node) =>
    create_card_reference(node.attrs!.title, node.attrs!.taskId),
  addNodeView() {
    return ({ node }) => {
      const dom = document.createElement("a");
      dom.href = `#card-${node.attrs.taskId}`;
      dom.dataset.cardReference = "";
      const icon = mount(Link2Icon, {
        target: dom,
        props: { class: "mr-1 inline-block size-[1em] align-[-0.125em]" },
      });
      dom.append(document.createTextNode(node.attrs.title));
      dom.contentEditable = "false";
      dom.addEventListener("click", (event) => {
        event.preventDefault();
        event.stopPropagation();
        window.dispatchEvent(
          new CustomEvent("cardbe:open-card", {
            detail: { taskId: node.attrs.taskId },
          }),
        );
      });
      const showPreview = () => {
        const { left, top, bottom } = dom.getBoundingClientRect();
        window.dispatchEvent(
          new CustomEvent("cardbe:show-card-preview", {
            detail: { taskId: node.attrs.taskId, left, top, bottom },
          }),
        );
      };
      const hidePreview = () =>
        window.dispatchEvent(new CustomEvent("cardbe:hide-card-preview"));
      dom.addEventListener("mouseenter", showPreview);
      dom.addEventListener("focus", showPreview);
      dom.addEventListener("mouseleave", hidePreview);
      dom.addEventListener("blur", hidePreview);
      return {
        dom,
        stopEvent: () => true,
        ignoreMutation: () => true,
        destroy: () => {
          hidePreview();
          void unmount(icon);
        },
      };
    };
  },
});
