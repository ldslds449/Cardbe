import { afterEach, describe, expect, it } from "vite-plus/test";
import {
  Editor,
  getExtensionField,
  createChainableState,
  type InputRule,
} from "@tiptap/core";
import { createEditorExtensions } from "./index";
import { serializeMarkdown } from "../serialization/markdown";
import { mathPasteKey } from "./math";
import { Slice } from "@tiptap/pm/model";

const editors: Editor[] = [];
afterEach(() => {
  editors.splice(0).forEach((editor) => editor.destroy());
});
function editor(content: string) {
  const instance = new Editor({
    element: null,
    extensions: createEditorExtensions(),
    content,
    contentType: "markdown",
  });
  editors.push(instance);
  return instance;
}
function context(instance: Editor) {
  const extension = instance.extensionManager.extensions.find(
    (extension) => extension.name === "blockMath",
  )!;
  return {
    extension,
    context: {
      editor: instance,
      name: extension.name,
      options: extension.options,
      storage: extension.storage,
      type: instance.schema.nodes.blockMath,
    },
  };
}
function enter(instance: Editor) {
  const ctx = context(instance);
  return getExtensionField<() => { Enter: () => boolean }>(
    ctx.extension,
    "addKeyboardShortcuts",
    ctx.context,
  )().Enter();
}

describe("display formulas in the rich editor", () => {
  it.each(["$$x^2$$", "$$\nx^2\n$$"])(
    "pastes a display formula as a block: %s",
    (source) => {
      const instance = editor("Before after");
      instance.commands.setTextSelection(8);
      const plugin = instance.extensionManager.plugins.find(
        (plugin) => plugin.spec.key === mathPasteKey,
      )!;
      const event = {
        clipboardData: {
          getData: (type: string) => (type === "text/plain" ? source : ""),
        },
      } as unknown as ClipboardEvent;
      expect(
        plugin.props.handlePaste!.call(
          plugin,
          instance.view,
          event,
          Slice.empty,
        ),
      ).toBe(true);
      expect(instance.getJSON().content?.map((node) => node.type)).toEqual([
        "paragraph",
        "blockMath",
        "paragraph",
      ]);
      expect(instance.state.doc.textContent).toBe("Before after");
      expect(instance.getJSON().content?.[1]).toMatchObject({
        type: "blockMath",
        attrs: { latex: "x^2" },
      });
    },
  );
  it("parses a single-line double-dollar formula as its own block", () => {
    const instance = editor("Before\n\n$$x^2$$\n\nAfter");
    expect(instance.getJSON().content?.map((node) => node.type)).toEqual([
      "paragraph",
      "blockMath",
      "paragraph",
    ]);
    expect(serializeMarkdown(instance)).toBe("Before\n\n$$\nx^2\n$$\n\nAfter");
  });

  it("marks double-dollar formulas within text for display rendering", () => {
    expect(
      editor("Before $$x^2$$ after").getJSON().content?.[0]?.content?.[1],
    ).toMatchObject({
      type: "inlineMath",
      attrs: { display: true, latex: "x^2" },
    });
  });

  it("opens an empty formula block after typing $$ and pressing Enter", () => {
    const instance = editor("$$");
    instance.commands.setTextSelection(3);
    expect(enter(instance)).toBe(true);
    expect(instance.getJSON().content?.[0]).toMatchObject({
      type: "blockMath",
      attrs: { latex: "" },
    });
    expect(editor(serializeMarkdown(instance)).getJSON()).toEqual(
      instance.getJSON(),
    );
  });

  it("leaves normal Enter, code, readonly content and required list paragraphs alone", () => {
    for (const [source, pos] of [
      ["normal", 7],
      ["```text\n$$\n```", 3],
      ["- $$", 4],
    ] as const) {
      const instance = editor(source);
      instance.commands.setTextSelection(pos);
      expect(enter(instance)).toBe(false);
    }
    const instance = editor("$$");
    instance.commands.setTextSelection(3);
    instance.setEditable(false, false);
    expect(enter(instance)).toBe(false);
  });

  it.each([false, true])(
    "converts typed $$x$$ without deleting trailing content (trailing=%s)",
    (trailing) => {
      const instance = editor("placeholder");
      instance.commands.setContent({
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [
              { type: "text", text: `$$x$$${trailing ? " after" : ""}` },
            ],
          },
        ],
      });
      const ctx = context(instance);
      const rule = getExtensionField<() => InputRule[]>(
        ctx.extension,
        "addInputRules",
        ctx.context,
      )()[0];
      const tr = instance.state.tr;
      const state = createChainableState({
        state: instance.state,
        transaction: tr,
      });
      const match = /^(\${2,})(?!\$)([^$\n]+)\1$/.exec("$$x$$")!;
      rule.handler({
        state,
        range: { from: 1, to: 6 },
        match,
        commands: instance.commands,
        chain: () => instance.chain(),
        can: () => instance.can(),
      });
      instance.view.dispatch(tr);
      if (trailing) {
        expect(instance.state.doc.textContent).toBe("$$x$$ after");
      } else {
        expect(instance.getJSON().content?.[0]).toMatchObject({
          type: "blockMath",
          attrs: { latex: "x" },
        });
      }
    },
  );
});
