import {
  Accessibility,
  AutoScroller,
  Cursor,
  Feedback,
  type DragDropManager,
} from "@dnd-kit/dom";

/**
 * Replaces dnd-kit's Cursor and PreventSelection plugins. Each inserts a
 * `* { … !important }` rule when a drag starts and removes it when it ends;
 * on a large board every insertion or removal restyles the whole document, on
 * top of the restyle dnd-kit's own drag stylesheet already causes: a visible
 * hitch as a card is picked up and dropped. Clearing the selection keeps text
 * from being selected without restyling anything, at the cost of the global
 * grabbing cursor.
 *
 * It extends Cursor only for dnd-kit's plugin type and immediately disposes
 * Cursor's own effect, which does nothing until a drag starts.
 */
class DragSelectionGuard extends Cursor {
  constructor(manager: DragDropManager) {
    super(manager);
    this.destroy();

    const clear_selection = () => window.getSelection()?.removeAllRanges();
    const start = () => {
      clear_selection();
      document.addEventListener("selectionchange", clear_selection, {
        capture: true,
      });
    };
    const end = () => {
      document.removeEventListener("selectionchange", clear_selection, {
        capture: true,
      });
    };
    const unsubscribe = [
      manager.monitor.addEventListener("dragstart", start),
      manager.monitor.addEventListener("dragend", end),
    ];
    this.destroy = () => {
      end();
      unsubscribe.forEach((listener) => listener());
    };
  }
}

/** dnd-kit's default plugins, without the document-wide drag styles. */
export const drag_plugins = [
  Accessibility,
  AutoScroller,
  Feedback,
  DragSelectionGuard,
];
