import { describe, expect, it } from "vite-plus/test";
import { Editor } from "@tiptap/core";

import { createEditorExtensions } from "./extensions";
import { documentPosition, sourceOffset } from "./source-position";

const markdown = [
  "# 任務說明",
  "",
  "Repeat this line.",
  "",
  "A **bold** word and `code` in 中文🚀 text.",
  "",
  "- first item",
  "- second item",
  "",
  "Repeat this line.",
  "",
  "```ts",
  "const answer = 42;",
  "```",
].join("\n");

function withEditor(run: (editor: Editor) => void) {
  const editor = new Editor({
    element: null,
    extensions: createEditorExtensions(),
    content: markdown,
    contentType: "markdown",
  });
  try {
    run(editor);
  } finally {
    editor.destroy();
  }
}

describe("source positions", () => {
  it.each([
    ["inside a heading", "任務", 0],
    ["in the second of repeated lines", "this line.", 1],
    ["after formatted text", " word and", 0],
    ["after emoji", " text.", 0],
    ["in a list item", "ond item", 0],
    ["inside a code block", "answer", 0],
  ])("maps the cursor %s both ways", (_name, text, skip) => {
    let offset = markdown.indexOf(text);
    for (let index = 0; index < skip; index += 1) {
      offset = markdown.indexOf(text, offset + 1);
    }
    withEditor((editor) => {
      const { doc } = editor.state;
      const position = documentPosition(doc, markdown, offset);
      expect(doc.textBetween(position, position + text.length)).toBe(text);
      expect(sourceOffset(doc, markdown, position)).toBe(offset);
    });
  });

  it("maps the end of a text block to the end of its line", () => {
    const offset = markdown.indexOf("\n\n- first");
    withEditor((editor) => {
      const { doc } = editor.state;
      const position = documentPosition(doc, markdown, offset);
      expect(doc.textBetween(position - 5, position)).toBe("text.");
      expect(sourceOffset(doc, markdown, position)).toBe(offset);
    });
  });
});
