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
    // Warm the grammar so cold regex compilation cannot hit Shiki's time limit.
    getHighlighter().codeToTokensBase(code, {
      lang: "typescript",
      theme: "cardbe",
      tokenizeTimeLimit: 0,
    });
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
        .codeToTokensBase(code, { lang: "typescript", theme: "cardbe" })
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

  it("re-highlights only changed blocks with the same result as a full pass", async () => {
    await loadHighlightLanguage("typescript");
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content:
        "intro\n\n```typescript\nconst a = 1;\n```\n\nmiddle\n\n```typescript\nlet b = 2;\n```\n",
      contentType: "markdown",
    });
    try {
      const plugin = editor.extensionManager.plugins.find(
        (plugin) => plugin.spec.key === codeHighlightKey,
      )!;
      const describe = (state: EditorState) =>
        codeHighlightKey
          .getState(state)!
          .find()
          .map((decoration) => [
            decoration.from,
            decoration.to,
            (decoration as unknown as { type: { attrs: unknown } }).type.attrs,
          ]);
      const fresh = (state: EditorState) =>
        describe(EditorState.create({ doc: state.doc, plugins: [plugin] }));
      let state = EditorState.create({
        schema: editor.schema,
        doc: editor.schema.nodeFromJSON(editor.getJSON()),
        plugins: [plugin],
      });
      const blocks = () => {
        const positions: number[] = [];
        state.doc.forEach((node, offset) => {
          if (node.type.name === "codeBlock") {
            positions.push(offset);
          }
        });
        return positions;
      };
      const edits = [
        // Typing inside the first code block and in a paragraph before it.
        () => state.tr.insertText(" + 1", blocks()[0] + 12),
        () => state.tr.insertText("more ", 1),
        // Changing a language, then turning a paragraph into code.
        () =>
          state.tr.setNodeMarkup(blocks()[1], undefined, { language: "text" }),
        () =>
          state.tr.setBlockType(1, 2, editor.schema.nodes.codeBlock, {
            language: "typescript",
          }),
        // Turning code back into a paragraph, then deleting a block.
        () => state.tr.setBlockType(1, 2, editor.schema.nodes.paragraph),
        () =>
          state.tr.delete(
            blocks()[0],
            blocks()[0] + state.doc.nodeAt(blocks()[0])!.nodeSize,
          ),
      ];
      for (const edit of edits) {
        state = state.apply(edit());
        expect(describe(state)).toEqual(fresh(state));
      }
    } finally {
      editor.destroy();
    }
  });
});
