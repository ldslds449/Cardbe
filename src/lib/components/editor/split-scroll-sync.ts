import { EditorView } from "@codemirror/view";
import {
  invert_anchors,
  map_scroll,
  scroll_anchors,
  type ScrollAnchor,
} from "./scroll-sync";

const VIEWPORT_SELECTOR = "[data-slot='scroll-area-viewport']";
const HEADING_SELECTOR = "h1, h2, h3, h4, h5, h6";

export type SplitPane = "editor" | "preview";

/**
 * Keeps a plain-text editor and its rendered preview at the same content,
 * aligning Markdown headings and interpolating between them. The pane the
 * user last pointed at, focused, or typed in drives the other one.
 */
export class SplitScrollSync {
  #anchors: ScrollAnchor[] | null = null;
  #driver: SplitPane = "editor";

  constructor(
    private readonly editor_root: () => HTMLElement | null,
    private readonly preview_viewport: () => HTMLElement | null,
    private readonly heading_offsets: () => readonly number[],
  ) {}

  invalidate() {
    this.#anchors = null;
  }

  drive(pane: SplitPane) {
    this.#driver = pane;
  }

  /** Handles a scroll event captured from either pane. */
  scrolled(pane: SplitPane, event: Event) {
    const parts = this.#parts();
    if (!parts || pane !== this.#driver) {
      return;
    }
    const source = pane === "editor" ? parts.editor : parts.preview;
    // Ignore nested scroll areas such as horizontally scrolling code blocks.
    if (event.target === source) {
      this.sync();
    }
  }

  /** Moves the driven pane to match the driving pane. */
  sync() {
    const parts = this.#parts();
    if (!parts) {
      return;
    }
    this.#anchors ??= this.#build(parts);
    if (this.#driver === "editor") {
      parts.preview.scrollTop = map_scroll(
        this.#anchors,
        parts.editor.scrollTop,
      );
    } else {
      parts.editor.scrollTop = map_scroll(
        invert_anchors(this.#anchors),
        parts.preview.scrollTop,
      );
    }
  }

  #parts() {
    const root = this.editor_root()?.querySelector<HTMLElement>(".cm-editor");
    const view = root ? EditorView.findFromDOM(root) : null;
    const editor = root?.closest<HTMLElement>(VIEWPORT_SELECTOR);
    const preview = this.preview_viewport();
    return view && editor && preview ? { view, editor, preview } : null;
  }

  #build({
    view,
    editor,
    preview,
  }: {
    view: EditorView;
    editor: HTMLElement;
    preview: HTMLElement;
  }): ScrollAnchor[] {
    const offsets = this.heading_offsets();
    const headings = preview.querySelectorAll(HEADING_SELECTOR);
    let pairs: ScrollAnchor[] = [];
    // The preview may still show older text; then fall back to proportions.
    if (offsets.length > 0 && headings.length === offsets.length) {
      const editor_base =
        view.documentTop -
        editor.getBoundingClientRect().top +
        editor.scrollTop;
      const preview_base =
        preview.getBoundingClientRect().top - preview.scrollTop;
      // Lines outside the rendered range use estimated heights.
      const length = view.state.doc.length;
      pairs = offsets.map((offset, index) => [
        editor_base + view.lineBlockAt(Math.min(offset, length)).top,
        headings[index].getBoundingClientRect().top - preview_base,
      ]);
    }
    return scroll_anchors(
      pairs,
      editor.scrollHeight - editor.clientHeight,
      preview.scrollHeight - preview.clientHeight,
    );
  }
}
