import { afterEach, describe, expect, it } from "vite-plus/test";
import { render } from "svelte/server";
import { Editor } from "@tiptap/core";
import { editorMode } from "./mode";
import RichTextEditor from "./rich-text-editor.svelte";
import EditorControls from "./editor-controls.svelte";
import MarkdownRenderer from "../../../routes/components/markdown_renderer.svelte";
import { createEditorExtensions } from "./extensions";
import { serializeMarkdown } from "./serialization/markdown";
import { m } from "#lib/paraglide/messages.js";

afterEach(() => editorMode.set("rich"));

function stableHtml(html: string) {
  return html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(
      /\b(id|aria-controls|aria-labelledby|aria-describedby)="[^"]*"/g,
      "$1",
    );
}

describe("shared editor preview", () => {
  it.each(["rich", "plain"] as const)(
    "offers separate preview controls only in plain mode: %s",
    (mode) => {
      editorMode.set(mode);
      const { body } = render(EditorControls);
      expect(body.includes(m.editor_preview())).toBe(mode === "plain");
    },
  );

  it("keeps rich mode editable even when a caller requests preview", () => {
    editorMode.set("rich");
    const { body } = render(RichTextEditor, {
      props: { value: "$x$", previewing: true, showControls: false },
    });
    expect(body).not.toContain('role="region"');
    expect(body).not.toContain(m.editor_preview_empty());
    expect(body).toContain("data-editor-body");
  });

  it("adds a copy button to code blocks in the preview", () => {
    const { body } = render(MarkdownRenderer, {
      props: { md: "```typescript\nconst n = 42;\n```" },
    });
    expect(body).toContain("data-code-block");
    expect(body).toContain(`aria-label="${m.markdown_copy_code()}"`);
  });

  it.each([
    [
      "links and images",
      "https://example.com/photo.png\n\n[Link](https://example.com/page)\n\n![Photo](https://example.com/photo.png)",
    ],
    ["formulas", "Inline $x^2$ and $$y^2$$\n\n$$z^2$$\n\n$$\n\\frac{1}{2}\n$$"],
    [
      "Mermaid and code",
      "```mermaid\nflowchart LR\n A --> B\n```\n\n```typescript\nconst n = 42;\n```",
    ],
    [
      "lists and tables",
      "- [x] Done\n- [ ] Waiting\n\n| Left | Right |\n| :--- | ---: |\n| A | B |",
    ],
    ["footnotes and HTML", "Text[^1] and <b>raw HTML</b>.\n\n[^1]: Footnote"],
    [
      "formatting and line breaks",
      "# Heading\n\n**Bold $x$** and _italic_.\nSoft line\nNext line  \nHard break.",
    ],
  ])("preserves preview after rich-text serialization: %s", (_name, source) => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    try {
      const output = (md: string) =>
        stableHtml(
          render(MarkdownRenderer, { props: { md }, idPrefix: "content" }).body,
        );
      expect(output(serializeMarkdown(editor))).toBe(output(source));
    } finally {
      editor.destroy();
    }
  });
});
