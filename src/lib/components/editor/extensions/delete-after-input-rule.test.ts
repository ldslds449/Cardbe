import { describe, expect, test } from "vite-plus/test";
import { Editor } from "@tiptap/core";
import type { EditorState, Transaction } from "@tiptap/pm/state";
import { createEditorExtensions } from ".";
import { serializeMarkdown } from "../serialization/markdown";

type Handler = (...args: unknown[]) => boolean;

// A headless editor has no view; this one runs the plugins' key and text
// input handlers the way ProseMirror's view would.
function createTypingEditor() {
  const editor = new Editor({
    extensions: createEditorExtensions(),
    content: "z",
    contentType: "markdown",
  });
  const internal = editor as unknown as {
    editorView: unknown;
    dispatchTransaction: (tr: Transaction) => void;
  };
  const view = {
    state: editor.state.reconfigure({
      plugins: editor.extensionManager.plugins,
    }) as EditorState,
    dispatch: (tr: Transaction) => internal.dispatchTransaction(tr),
    updateState(state: EditorState) {
      view.state = state;
    },
    composing: false,
    editable: true,
    isDestroyed: false,
    hasFocus: () => true,
    endOfTextblock(direction: string) {
      const { $head } = view.state.selection;
      return direction === "backward" || direction === "left"
        ? $head.parentOffset === 0
        : $head.parentOffset === $head.parent.content.size;
    },
    someProp(name: string, call: (handler: Handler) => boolean) {
      for (const plugin of view.state.plugins) {
        const handler = (plugin.props as Record<string, Handler | undefined>)[
          name
        ];
        if (handler && call(handler.bind(plugin))) {
          return true;
        }
      }
      return false;
    },
  };
  internal.editorView = view;
  editor.commands.setContent("", { contentType: "markdown" });

  return {
    editor,
    type(text: string) {
      for (const character of text) {
        const { from, to } = view.state.selection;
        if (
          !view.someProp("handleTextInput", (handler) =>
            handler(view, from, to, character, () =>
              view.state.tr.insertText(character, from, to),
            ),
          )
        ) {
          view.dispatch(view.state.tr.insertText(character, from, to));
        }
      }
    },
    backspace() {
      const event = {
        key: "Backspace",
        keyCode: 8,
        altKey: false,
        ctrlKey: false,
        metaKey: false,
        shiftKey: false,
        preventDefault() {},
      };
      view.someProp("handleKeyDown", (handler) => handler(view, event));
    },
  };
}

describe("Backspace after a Markdown shortcut", () => {
  test.each(["**ab**", "*ab*", "`ab`", "~~ab~~", "$x$", "![a](a.png)"])(
    "does not restore the typed %s syntax",
    (syntax) => {
      const { editor, type, backspace } = createTypingEditor();
      type(syntax);
      const converted = serializeMarkdown(editor);
      backspace();

      expect(serializeMarkdown(editor)).toBe(converted);
    },
  );

  test("removes a list started by its shortcut", () => {
    const { editor, type, backspace } = createTypingEditor();
    type("- ");
    backspace();

    expect(editor.getText().trim()).toBe("");
    expect(JSON.stringify(editor.getJSON())).not.toContain("bulletList");
  });
});
