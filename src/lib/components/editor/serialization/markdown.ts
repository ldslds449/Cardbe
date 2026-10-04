import type { Editor } from "@tiptap/core";

export function markdownInsertionContent(editor: Editor, markdown: string) {
  const blocks = editor.markdown!.parse(markdown).content ?? [];
  // A single paragraph is inline content when inserted at the cursor.
  return blocks.length === 1 && blocks[0].type === "paragraph"
    ? (blocks[0].content ?? [])
    : blocks;
}

export function serializeMarkdown(editor: Editor): string {
  return editor.getMarkdown();
}
