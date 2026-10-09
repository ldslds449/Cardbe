import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { Node } from "@tiptap/pm/model";
import { bundledLanguages } from "shiki/langs";
import { logger } from "$lib/logger";
import {
  getHighlighter,
  loadHighlightLanguage,
} from "../../../../routes/components/highlighter.svelte";

export const codeLanguages = Object.keys(bundledLanguages).sort();
export const codeHighlightKey = new PluginKey<DecorationSet>("code-highlight");

export const CodeHighlight = Extension.create({
  name: "codeHighlight",
  addProseMirrorPlugins() {
    const editor = this.editor;
    const pending = new Set<Promise<void>>();
    const highlight = (doc: Node) => {
      const decorations: Decoration[] = [];
      doc.descendants((node, position) => {
        if (node.type.name !== "codeBlock") {
          return;
        }
        const language = node.attrs.language || "text";
        const load = loadHighlightLanguage(language);
        if (load && !pending.has(load)) {
          pending.add(load);
          void load
            .then(() => {
              if (!editor.isDestroyed) {
                editor.view.dispatch(
                  editor.state.tr.setMeta(codeHighlightKey, true),
                );
              }
            })
            .catch((error) =>
              logger.warn("editor.highlight_load.failed", error),
            )
            .finally(() => pending.delete(load));
        }
        const highlighter = getHighlighter();
        const tokens = highlighter.codeToTokensBase(node.textContent, {
          lang: highlighter.getLoadedLanguages().includes(language)
            ? language
            : "text",
          theme: "cardbe",
        });
        let offset = position + 1;
        for (const line of tokens) {
          for (const token of line) {
            if (token.content.length && token.color) {
              decorations.push(
                Decoration.inline(offset, offset + token.content.length, {
                  style: `color: ${token.color}`,
                }),
              );
            }
            offset += token.content.length;
          }
          offset += 1;
        }
      });
      return DecorationSet.create(doc, decorations);
    };
    return [
      new Plugin({
        key: codeHighlightKey,
        state: {
          init: (_, state) => highlight(state.doc),
          apply: (transaction, decorations) =>
            transaction.docChanged || transaction.getMeta(codeHighlightKey)
              ? highlight(transaction.doc)
              : decorations.map(transaction.mapping, transaction.doc),
        },
        props: { decorations: (state) => codeHighlightKey.getState(state) },
      }),
    ];
  },
});
