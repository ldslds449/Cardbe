import { describe, expect, it } from "vite-plus/test";
import { Editor } from "@tiptap/core";
import { EditorState } from "@tiptap/pm/state";
import { Decoration } from "@tiptap/pm/view";
import { createEditorExtensions } from "./index";
import { codeHighlightKey } from "./code-highlight";
import {
  getHighlighter,
  loadHighlightLanguage,
} from "../../../../routes/components/highlighter.svelte";

describe("editor code highlighting", () => {
  it("uses shared Shiki colors and keeps token positions across lines", async () => {
    await loadHighlightLanguage("typescript");
    const code = "const answer = 42;\n// 中文";
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: `\`\`\`typescript\n${code}\n\`\`\``,
      contentType: "markdown",
    });
    try {
      expect(editor.getJSON().content?.[0]?.attrs?.language).toBe("typescript");
      const plugin = editor.extensionManager.plugins.find(
        (plugin) => plugin.spec.key === codeHighlightKey,
      )!;
      const state = EditorState.create({
        schema: editor.schema,
        doc: editor.schema.nodeFromJSON(editor.getJSON()),
        plugins: [plugin],
      });
      const decorations = codeHighlightKey.getState(state)!.find();
      const tokens = getHighlighter()
        .codeToTokensBase(code, { lang: "typescript", theme: "github-dark" })
        .flat();
      expect(decorations).toHaveLength(
        tokens.filter((token) => token.content.length && token.color).length,
      );
      decorations.forEach((decoration, index) => {
        expect(state.doc.textBetween(decoration.from, decoration.to)).toBe(
          tokens[index].content,
        );
        expect(decoration).toEqual(
          Decoration.inline(decoration.from, decoration.to, {
            style: `color: ${tokens[index].color}`,
          }),
        );
      });
      const refreshed = state.apply(state.tr.setMeta(codeHighlightKey, true));
      expect(refreshed.doc.eq(state.doc)).toBe(true);
      expect(codeHighlightKey.getState(refreshed)!.find()).toHaveLength(
        decorations.length,
      );
    } finally {
      editor.destroy();
    }
  });
});
