import { afterEach, describe, expect, it } from "vite-plus/test";
import { Editor } from "@tiptap/core";
import { Fragment, Slice } from "@tiptap/pm/model";
import { history } from "@tiptap/pm/history";
import { createEditorExtensions } from "./index";
import { serializeMarkdown } from "../serialization/markdown";

const editors: Editor[] = [];
afterEach(() => {
  editors.splice(0).forEach((editor) => editor.destroy());
});
function editor(content = "") {
  const instance = new Editor({
    element: null,
    extensions: createEditorExtensions(),
    content: content || { type: "doc", content: [{ type: "paragraph" }] },
    contentType: "markdown",
  });
  editors.push(instance);
  instance.registerPlugin(history());
  return instance;
}
function clipboard(text: string) {
  return {
    clipboardData: {
      getData: (type: string) => (type === "text/plain" ? text : ""),
      files: { length: 0 },
    },
  } as unknown as ClipboardEvent;
}
const urls = [
  "https://example.com/page",
  "https://example.com/photo.png?token=abc",
  "https://example.com/photo.JPG",
  "https://example.com/image?id=1",
];

describe("URL paste and explicit images", () => {
  it.each(urls)(
    "links selected text without replacing it with an image: %s",
    (url) => {
      const instance = editor("Selected text");
      instance.commands.setTextSelection({ from: 1, to: 14 });
      const slice = new Slice(Fragment.from(instance.schema.text(url)), 0, 0);
      const handled = instance.extensionManager.plugins.some((plugin) =>
        plugin.props.handlePaste?.call(
          plugin,
          instance.view,
          clipboard(url),
          slice,
        ),
      );
      expect(handled).toBe(true);
      expect(serializeMarkdown(instance)).toBe(`[Selected text](${url})`);
    },
  );

  it.each(urls)("pastes a URL as a link at the caret: %s", (url) => {
    const instance = editor();
    const event = clipboard(url);
    const slice = new Slice(Fragment.from(instance.schema.text(url)), 0, 0);
    const plugins = instance.extensionManager.plugins;
    expect(
      plugins.some((plugin) =>
        plugin.props.handlePaste?.call(plugin, instance.view, event, slice),
      ),
    ).toBe(false);
    for (const plugin of plugins) {
      if (
        plugin.props.handleDOMEvents?.paste &&
        plugin.spec.appendTransaction
      ) {
        instance.registerPlugin(plugin);
        plugin.props.handleDOMEvents.paste.call(plugin, instance.view, event);
      }
    }
    instance.view.dispatch(
      instance.state.tr.insertText(url).setMeta("uiEvent", "paste"),
    );
    expect(serializeMarkdown(instance)).toBe(`[${url}](${url})`);
    expect(instance.getJSON().content?.[0]?.content?.[0]?.type).toBe("text");
    expect(instance.commands.undo()).toBe(true);
    expect(serializeMarkdown(instance)).toBe("");
  });

  it("preserves explicitly inserted images and their Markdown", () => {
    const instance = editor();
    const src = "https://example.com/photo.png?token=abc";
    expect(instance.commands.setImage({ src, alt: "Photo" })).toBe(true);
    const markdown = serializeMarkdown(instance);
    expect(markdown).toBe(`![Photo](${src})`);
    expect(editor(markdown).getJSON()).toEqual(instance.getJSON());
  });
});
