/**
 * A width resized by dragging or the arrow keys and remembered in local
 * storage. `grow` is the pointer direction that widens it: -1 for a handle on
 * the start edge (a panel docked at the end), 1 for one on the end edge.
 */
export class StoredWidth {
  value = $state(0);

  constructor(
    private readonly key: string,
    readonly fallback: number,
    readonly min: number,
    readonly max: number,
    private readonly grow: 1 | -1,
  ) {
    this.value = this.read();
  }

  private clamp(width: number) {
    return Math.min(this.max, Math.max(this.min, width));
  }

  private read() {
    try {
      const stored = Number(window.localStorage.getItem(this.key));
      if (Number.isFinite(stored) && stored > 0) {
        return this.clamp(stored);
      }
    } catch {
      // Keep the default width when storage is unavailable.
    }
    return this.fallback;
  }

  set(width: number, persist = true) {
    this.value = this.clamp(width);
    if (!persist) {
      return;
    }
    try {
      window.localStorage.setItem(this.key, String(this.value));
    } catch {
      // Resizing still works when storage is unavailable.
    }
  }

  reset() {
    this.set(this.fallback);
  }

  start(event: PointerEvent) {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    const handle = event.currentTarget as HTMLElement;
    const start_x = event.clientX;
    const start_width = this.value;
    const move = (move_event: PointerEvent) =>
      this.set(start_width + this.grow * (move_event.clientX - start_x), false);
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("lostpointercapture", stop);
      document.documentElement.removeAttribute("data-panel-resizing");
      this.set(this.value);
    };
    handle.setPointerCapture(event.pointerId);
    handle.addEventListener("pointermove", move);
    handle.addEventListener("lostpointercapture", stop);
    document.documentElement.setAttribute("data-panel-resizing", "");
  }

  keydown(event: KeyboardEvent) {
    const step = event.shiftKey ? 48 : 16;
    const widths: Record<string, number> = {
      Home: this.min,
      End: this.max,
      ArrowLeft: this.value - this.grow * step,
      ArrowRight: this.value + this.grow * step,
    };
    if (event.key in widths) {
      event.preventDefault();
      this.set(widths[event.key]);
    }
  }
}
