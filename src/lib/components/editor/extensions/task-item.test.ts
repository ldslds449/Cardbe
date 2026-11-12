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
