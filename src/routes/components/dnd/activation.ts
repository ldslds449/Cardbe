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
