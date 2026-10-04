import Link from "@tiptap/extension-link";
import Placeholder from "@tiptap/extension-placeholder";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { CodeHighlight } from "./code-highlight";
import { CodeScroll } from "./code-scroll";
import { CardReference } from "./card-reference";
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

export function createEditorExtensions(placeholder = "Start writing…") {
  return [
    StarterKit.configure({
      link: false,
    }),
    CodeHighlight,
    CodeScroll,
    CardReference,
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
