import { describe, expect, it } from "vite-plus/test";
import { Editor } from "@tiptap/core";
import type { TaskItemOptions } from "@tiptap/extension-task-item";
import { createEditorExtensions } from "./index";
import { m } from "#lib/paraglide/messages.js";

describe("task items", () => {
  it("labels checkboxes without repeating the item text", () => {
    const instance = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "- [ ] 項目一\n- [x] 項目二ABC",
      contentType: "markdown",
    });
    const options = instance.extensionManager.extensions.find(
      (extension) => extension.name === "taskItem",
    )!.options as TaskItemOptions;
    const labels: string[] = [];
    instance.state.doc.descendants((node) => {
      if (node.type.name === "taskItem") {
        labels.push(options.a11y!.checkboxLabel!(node, node.attrs.checked));
      }
    });
    instance.destroy();

    // Text that changes while typing misplaces Windows IME input.
    expect(labels).toEqual([
      m.editor_task_item_checkbox(),
      m.editor_task_item_checkbox(),
    ]);
  });
});

describe("task item input rule", () => {
  // Each item lists its paragraphs.
  function type_after(items: string[][], text: string) {
    const instance = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: {
        type: "doc",
        content: [
          {
            type: "bulletList",
            content: items.map((paragraphs) => ({
              type: "listItem",
              content: paragraphs.map((value) => ({
                type: "paragraph",
                content: [{ type: "text", text: value }],
              })),
            })),
          },
        ],
      },
    });
    const end = instance.state.doc.content.size - 2;
    instance.commands.setTextSelection(end);
    const { from, to } = instance.state.selection;
    const handled = instance.extensionManager.plugins.some((plugin) =>
      plugin.props.handleTextInput?.call(
        plugin,
        instance.view,
        from,
        to,
        text,
        () => instance.state.tr,
      ),
    );
    if (!handled) {
      instance.commands.insertContent(text);
    }
    const result = instance.getMarkdown();
    instance.destroy();
    return result.trim();
  }

  it("turns a bullet item into a task item", () => {
    expect(type_after([["[ ]"]], " ")).toBe("- [ ]");
    expect(type_after([["[X]"]], " ")).toBe("- [x]");
  });

  it("converts only the typed bullet item", () => {
    expect(type_after([["one"], ["[ ]"]], " ")).toBe("- one\n\n- [ ]");
  });

  it("nests a task list after the first paragraph of a bullet item", () => {
    expect(type_after([["one", "[ ]"]], " ")).toBe("- one\n  - [ ]");
  });
});
