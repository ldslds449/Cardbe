import Image, { inputRegex } from "@tiptap/extension-image";
import { InputRule } from "@tiptap/core";
import { mount, unmount } from "svelte";
import EditorImage from "../editor-image.svelte";

export const EditorImageExtension = Image.extend({
  addInputRules() {
    return [
      // Tiptap's node input rule inserts the typed `)` again after typing
      // already inserted it, leaving a stray `)` after the image.
      new InputRule({
        find: inputRegex,
        handler: ({ state, range, match }) => {
          const [, image, alt, src, title] = match;
          state.tr.replaceWith(
            range.to - image.length,
            range.to,
            this.type.create({ src, alt, title }),
          );
        },
      }),
    ];
  },
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
