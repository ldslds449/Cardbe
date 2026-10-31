import { Extension } from "@tiptap/core";
import { Plugin, PluginKey, type Transaction } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { Node } from "@tiptap/pm/model";
import { bundledLanguages } from "shiki/langs";
import { logger } from "#lib/logger.js";
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
    const highlightBlock = (
      node: Node,
      position: number,
      decorations: Decoration[],
    ) => {
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
          .catch((error) => logger.warn("editor.highlight_load.failed", error))
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
    };
    const highlight = (doc: Node) => {
      const decorations: Decoration[] = [];
      doc.descendants((node, position) => {
        if (node.type.name === "codeBlock") {
          highlightBlock(node, position, decorations);
        }
      });
      return DecorationSet.create(doc, decorations);
    };
    // Tokenizing every code block on each keystroke makes long documents
    // lag; only the text blocks a transaction touched are highlighted again.
    const rehighlightChanged = (
      transaction: Transaction,
      previous: DecorationSet,
    ) => {
      const { doc, mapping } = transaction;
      const blocks = new Map<number, Node>();
      mapping.maps.forEach((map, index) => {
        const rest = mapping.slice(index + 1);
        map.forEach((_oldStart, _oldEnd, newStart, newEnd) => {
          const from = Math.max(0, rest.map(newStart, -1) - 1);
          const to = Math.min(doc.content.size, rest.map(newEnd, 1) + 1);
          doc.nodesBetween(from, to, (node, position) => {
            if (node.isTextblock) {
              blocks.set(position, node);
              return false;
            }
          });
        });
      });
      let decorations = previous.map(mapping, doc);
      const added: Decoration[] = [];
      for (const [position, node] of blocks) {
        decorations = decorations.remove(
          decorations.find(position, position + node.nodeSize),
        );
        if (node.type.name === "codeBlock") {
          highlightBlock(node, position, added);
        }
      }
      return decorations.add(doc, added);
    };
    return [
      new Plugin({
        key: codeHighlightKey,
        state: {
          init: (_, state) => highlight(state.doc),
          apply: (transaction, decorations) =>
            transaction.getMeta(codeHighlightKey)
              ? highlight(transaction.doc)
              : transaction.docChanged
                ? rehighlightChanged(transaction, decorations)
                : decorations,
        },
        props: { decorations: (state) => codeHighlightKey.getState(state) },
      }),
    ];
  },
});
