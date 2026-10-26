import { afterEach, describe, expect, it } from "vite-plus/test";
import { Editor } from "@tiptap/core";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { createEditorExtensions } from "./index";
import { CardReferenceSuggestionKey } from "./card-reference";

const editors: Editor[] = [];
afterEach(() => {
  editors.splice(0).forEach((editor) => editor.destroy());
});

// An unmounted editor registers no plugins, so drive its plugins directly.
function suggestionAfterTyping(
  text: string,
  {
    enabled = true,
    withSuggestion = true,
    block = "paragraph",
    marks = [] as string[],
  } = {},
) {
  const editor = new Editor({
    element: null,
    extensions: createEditorExtensions(
      undefined,
      withSuggestion ? { allow: () => enabled } : null,
    ),
    content: { type: "doc", content: [{ type: block }] },
  });
  editors.push(editor);
  const { schema } = editor;
  const empty = EditorState.create({
    doc: editor.state.doc,
    plugins: editor.extensionManager.plugins,
  });
  const tr = empty.tr.insert(
    1,
    schema.text(
      text,
      marks.map((mark) => schema.marks[mark].create()),
    ),
  );
  tr.setSelection(TextSelection.create(tr.doc, 1 + text.length));
  return CardReferenceSuggestionKey.getState(empty.apply(tr));
}

describe("card reference suggestion", () => {
  it("opens immediately after [[ without requiring any card items", () => {
    expect(suggestionAfterTyping("[[")).toMatchObject({
      active: true,
      query: "",
    });
  });

  it("tracks the query typed after [[, including spaces", () => {
    expect(suggestionAfterTyping("See[[My ca")).toMatchObject({
      active: true,
      query: "My ca",
      range: { from: 4, to: 11 },
    });
  });

  it("stays inactive when the caller disables it", () => {
    expect(suggestionAfterTyping("[[My", { enabled: false })?.active).toBe(
      false,
    );
  });

  it("ignores references typed out by hand", () => {
    expect(suggestionAfterTyping("[[Card|task_1]]")?.active).toBe(false);
  });

  it("ignores [[ inside code", () => {
    expect(suggestionAfterTyping("[[My", { marks: ["code"] })?.active).toBe(
      false,
    );
    expect(suggestionAfterTyping("[[My", { block: "codeBlock" })?.active).toBe(
      false,
    );
  });

  it("is not installed without a suggestion renderer", () => {
    expect(
      suggestionAfterTyping("[[My", { withSuggestion: false }),
    ).toBeUndefined();
  });
});
