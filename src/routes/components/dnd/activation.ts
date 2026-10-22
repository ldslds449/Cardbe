// Pointer drags start only after moving `value` pixels, so plain clicks still work.
export function distance_activation_constraint(value: number) {
  let origin: { x: number; y: number } | undefined;
  return {
    controller: undefined as
      | { activate: (event: PointerEvent) => void }
      | undefined,
    onEvent(event: PointerEvent) {
      if (event.type === "pointerdown") {
        if (
          event.target instanceof Element &&
          event.target.closest('[data-slot="scroll-area-scrollbar"]')
        ) {
          this.abort();
          return;
        }
        origin = { x: event.clientX, y: event.clientY };
      } else if (
        event.type === "pointermove" &&
        origin &&
        Math.hypot(event.clientX - origin.x, event.clientY - origin.y) >= value
      ) {
        this.controller?.activate(event);
      } else if (event.type === "pointerup" || event.type === "pointercancel") {
        this.abort();
      }
    },
    abort() {
      origin = undefined;
    },
  };
}

// dnd-kit's default interactive targets, plus regions marked `data-no-drag`
// (rendered code blocks and tables) whose text can be selected. Unlike
// stopping propagation, this still lets open menus see the outside click.
const NO_DRAG_SELECTOR = [
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "button:not([disabled])",
  "a[href]",
  '[contenteditable]:not([contenteditable="false"])',
  "[data-no-drag]",
].join(", ");

export function prevent_drag_activation(
  event: Event,
  source: { element?: Element; handle?: Element },
): boolean {
  const target = event.target;
  if (
    !(target instanceof Element) ||
    target === source.element ||
    target === source.handle ||
    source.handle?.contains(target)
  ) {
    return false;
  }
  return target.closest(NO_DRAG_SELECTOR) !== null;
}
