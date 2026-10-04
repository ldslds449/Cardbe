import { Extension } from "@tiptap/core";
import { Plugin } from "@tiptap/pm/state";
import { flushSync, mount, unmount } from "svelte";
import CodeBlockView from "../code-block-view.svelte";

export const CodeScroll = Extension.create({
  name: "codeScroll",
  addProseMirrorPlugins() {
    return [
      new Plugin({
        props: {
          nodeViews: {
            codeBlock: () => {
              const dom = document.createElement("div");
              let contentDOM: HTMLElement | null = null;
              const component = flushSync(() =>
                mount(CodeBlockView, {
                  target: dom,
                  props: {
                    get contentDOM() {
                      return contentDOM;
                    },
                    set contentDOM(value) {
                      contentDOM = value;
                    },
                  },
                }),
              );
              return {
                dom,
                contentDOM: contentDOM!,
                stopEvent: (event) =>
                  event.target instanceof Element &&
                  !!event.target.closest('[data-slot="scroll-area-scrollbar"]'),
                ignoreMutation: (mutation) =>
                  mutation.type !== "selection" &&
                  !contentDOM!.contains(mutation.target),
                destroy: () => {
                  void unmount(component);
                },
              };
            },
          },
        },
      }),
    ];
  },
});
