import { Node } from "@tiptap/core";
import { mount, unmount } from "svelte";
import Link2Icon from "@lucide/svelte/icons/link-2";
import {
  create_card_reference,
  task_id_from_card_reference,
} from "../../../../routes/utils/card-reference";

export const CardReference = Node.create({
  name: "cardReference",
  group: "inline",
  inline: true,
  atom: true,
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
      const match = /^\[\[([^\]\n|]+)\|(task_\d+)\]\]/.exec(source);
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
