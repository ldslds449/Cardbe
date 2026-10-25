import Image from "@tiptap/extension-image";
import { mount, unmount } from "svelte";
import EditorImage from "../editor-image.svelte";

export const EditorImageExtension = Image.extend({
  addNodeView() {
    return ({ node }) => {
      const dom = document.createElement("span");
      dom.className = "editor-image";
      const component = mount(EditorImage, { target: dom, props: node.attrs });
      return {
        dom,
        ignoreMutation: () => true,
        destroy: () => {
          void unmount(component);
        },
      };
    };
  },
});
