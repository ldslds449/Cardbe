import { tick } from "svelte";
import type { Attachment } from "svelte/attachments";

/**
 * Windows long lists of heavy items (menus, drag registration): an item mounts
 * when its placeholder nears the scroll viewport and unmounts again once it is
 * far away, leaving a placeholder of its measured height. Items holding focus,
 * or any while a dialog, menu, or drag is open, stay mounted.
 */
export class LazyMount {
  // A record, unlike a SvelteSet, tracks each lookup by its own key.
  #mounted = $state<Record<string, true>>({});
  #heights = new Map<string, number>();
  #placeholders = new Map<Element, string>();
  #items = new Map<Element, string>();
  // Reusing an item's attachment keeps it from being observed again, with a
  // fresh unmount check, whenever its owner re-renders.
  #item_attachments = new Map<string, Attachment<HTMLElement>>();
  #mount_observer: IntersectionObserver | null = null;
  #unmount_observer: IntersectionObserver | null = null;

  constructor(
    private readonly mount_margin = "400px",
    private readonly unmount_margin = "1200px",
  ) {}

  is_mounted(id: string): boolean {
    return this.#mounted[id] === true;
  }

  mount(id: string) {
    this.#mounted[id] = true;
  }

  /** The height an unmounted item last had, for its placeholder. */
  height(id: string): number | undefined {
    return this.#heights.get(id);
  }

  /** Mounts the item once its placeholder nears the enclosing ScrollArea. */
  placeholder(id: string): Attachment<HTMLElement> {
    return (node) => {
      this.#mount_observer ??= new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            const target = this.#placeholders.get(entry.target);
            if (entry.isIntersecting && target !== undefined) {
              this.mount(target);
            }
          }
        },
        { root: scroll_root(node), rootMargin: this.mount_margin },
      );
      this.#placeholders.set(node, id);
      this.#mount_observer.observe(node);
      return () => {
        this.#placeholders.delete(node);
        this.#mount_observer?.unobserve(node);
      };
    };
  }

  /** Unmounts the mounted item once it is far outside the ScrollArea. */
  item(id: string): Attachment<HTMLElement> {
    const existing = this.#item_attachments.get(id);
    if (existing) {
      return existing;
    }
    const attachment: Attachment<HTMLElement> = (node) => {
      this.#unmount_observer ??= new IntersectionObserver(
        (entries) => {
          if (document.querySelector(KEEP_MOUNTED)) {
            return;
          }
          for (const entry of entries) {
            const target = this.#items.get(entry.target);
            if (
              !entry.isIntersecting &&
              target !== undefined &&
              !entry.target.contains(document.activeElement)
            ) {
              this.#heights.set(target, entry.boundingClientRect.height);
              delete this.#mounted[target];
            }
          }
        },
        { root: scroll_root(node), rootMargin: this.unmount_margin },
      );
      this.#items.set(node, id);
      this.#unmount_observer.observe(node);
      return () => {
        this.#items.delete(node);
        this.#unmount_observer?.unobserve(node);
      };
    };
    this.#item_attachments.set(id, attachment);
    return attachment;
  }

  /**
   * Keyboard navigation reaches placeholders too: mounts the item and moves
   * focus to the element `focus_target` picks from the item that replaced it.
   */
  async mount_focused(
    event: FocusEvent,
    id: string,
    focus_target: (item: Element) => HTMLElement | null,
  ) {
    const placeholder = event.currentTarget as HTMLElement;
    const parent = placeholder.parentElement;
    const previous = placeholder.previousElementSibling;
    this.mount(id);
    await tick();
    const item = previous
      ? previous.nextElementSibling
      : parent?.firstElementChild;
    if (item) {
      focus_target(item)?.focus();
    }
  }

  destroy() {
    this.#mount_observer?.disconnect();
    this.#unmount_observer?.disconnect();
    this.#mount_observer = null;
    this.#unmount_observer = null;
  }
}

// Open dialogs and menus belong to the item that opened them. A dragged item
// moves to the top layer, where it no longer intersects the ScrollArea, and
// unmounting it would cancel the drag.
const KEEP_MOUNTED =
  '[role="dialog"], [role="alertdialog"], [role="menu"], [data-dnd-dragging]';

function scroll_root(node: Element): Element | null {
  return node.closest('[data-slot="scroll-area-viewport"]');
}
