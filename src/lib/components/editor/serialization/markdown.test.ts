import { afterEach, describe, expect, it } from "vite-plus/test";
import { Editor } from "@tiptap/core";

import { createEditorExtensions } from "../extensions";
import { codeLanguages } from "../extensions/code-highlight";
import { markdownInsertionContent, serializeMarkdown } from "./markdown";

const markdown = [
  "# Heading",
  "",
  "A **bold** and _italic_ ~~struck~~ [link](https://example.com) with `code`.",
  "",
  "- bullet",
  "- [x] done",
  "",
  "1. ordered",
  "",
  "> quote",
  "",
  "```rust",
  "fn main() {",
  '    println!("hello");',
  "}",
  "```",
  "",
  "---",
].join("\n");

const editors: Editor[] = [];

afterEach(() => {
  for (const editor of editors.splice(0)) {
    editor.destroy();
  }
});

describe("Markdown editor serialization", () => {
  it("accepts link destinations without requiring an absolute URL", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Selected text",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 14 });
    for (const href of [
      "example.com",
      "../notes",
      "#section",
      "mailto:hello@example.com",
    ]) {
      expect(editor.chain().setLink({ href }).run()).toBe(true);
      expect(editor.getAttributes("link").href).toBe(href);
    }
  });

  it("rejects unsafe URLs without changing the selected text", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Selected text",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 14 });
    expect(editor.chain().setLink({ href: "javascript:alert(1)" }).run()).toBe(
      false,
    );
    expect(serializeMarkdown(editor)).toBe("Selected text");
  });

  it("adds a link to the saved selection after focus moves to the URL dialog", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Selected text and other text.",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 14 });
    const selection = {
      from: editor.state.selection.from,
      to: editor.state.selection.to,
    };
    editor.commands.setTextSelection(20);
    editor
      .chain()
      .setTextSelection(selection)
      .setLink({ href: "https://example.com" })
      .run();
    expect(serializeMarkdown(editor)).toBe(
      "[Selected text](https://example.com) and other text.",
    );
    editor.commands.unsetLink();
    expect(serializeMarkdown(editor)).toBe("Selected text and other text.");
  });

  it("serializes a changed code language without changing the code", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "```javascript\nconst value = 1;\n```",
      contentType: "markdown",
    });
    editors.push(editor);
    expect(codeLanguages).toContain("typescript");
    editor.commands.setTextSelection(1);
    editor.commands.updateAttributes("codeBlock", { language: "typescript" });
    expect(serializeMarkdown(editor)).toBe(
      "```typescript\nconst value = 1;\n```",
    );
  });
  it("imports and exports the supported Markdown blocks without losing language metadata", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: markdown,
      contentType: "markdown",
    });
    editors.push(editor);

    const document = editor.getJSON();
    expect(document.content?.map((node) => node.type)).toEqual([
      "heading",
      "paragraph",
      "bulletList",
      "taskList",
      "orderedList",
      "blockquote",
      "codeBlock",
      "horizontalRule",
    ]);
    expect(document.content?.[6]?.attrs?.language).toBe("rust");

    const output = serializeMarkdown(editor);
    expect(output).toContain("# Heading");
    expect(output).toContain("**bold**");
    expect(output).toContain("- [x] done");
    expect(output).toContain("```rust");
    expect(output).toContain('println!("hello");');
    expect(output).toContain("---");
  });

  it("keeps an empty document editable", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: { type: "doc", content: [{ type: "paragraph" }] },
    });
    editors.push(editor);

    expect(editor.isEmpty).toBe(true);
    expect(serializeMarkdown(editor)).toBe("");
  });

  it("preserves card references through Markdown serialization", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "See [[My card|task_123]] and [ordinary](https://example.com).",
      contentType: "markdown",
    });
    editors.push(editor);
    expect(editor.getJSON().content?.[0]?.content?.[1]).toMatchObject({
      type: "cardReference",
      attrs: { title: "My card", taskId: "task_123" },
    });
    expect(serializeMarkdown(editor)).toBe(
      "See [[My card|task_123]] and [ordinary](https://example.com).",
    );
  });
  it("inserts a card reference inline without splitting its paragraph", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Before [[ after",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.insertContentAt(
      { from: 8, to: 10 },
      markdownInsertionContent(editor, "[[Card|task_123]]"),
    );
    expect(editor.getJSON().content).toHaveLength(1);
    expect(serializeMarkdown(editor)).toBe("Before [[Card|task_123]] after");
  });
  it("keeps card reference syntax literal inside code", () => {
    const source = "`[[Card|task_123]]`\n\n```text\n[[Card|task_123]]\n```";
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(editor.getJSON().content?.[0]?.content?.[0]?.type).toBe("text");
    expect(serializeMarkdown(editor)).toBe(source);
  });
  it("keeps multiple Markdown paragraphs as blocks when inserting", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: { type: "doc", content: [{ type: "paragraph" }] },
    });
    editors.push(editor);
    editor.commands.insertContentAt(
      1,
      markdownInsertionContent(editor, "First\n\nSecond"),
    );
    expect(serializeMarkdown(editor)).toBe("First\n\nSecond");
  });
});
