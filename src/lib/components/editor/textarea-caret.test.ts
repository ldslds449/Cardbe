import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { caretRect } from "./textarea-caret";

afterEach(() => vi.unstubAllGlobals());

describe("plain editor reference anchor", () => {
  it("translates the mirrored caret into viewport coordinates and removes the mirror", () => {
    const remove = vi.fn();
    const mirror = {
      style: { setProperty: vi.fn() },
      setAttribute: vi.fn(),
      append: vi.fn(),
      remove,
      getBoundingClientRect: () => ({ left: 10, top: 20 }),
    };
    const marker = {
      getBoundingClientRect: () => ({ left: 70, top: 60, height: 18 }),
    };
    vi.stubGlobal("document", {
      createElement: (tag: string) => (tag === "div" ? mirror : marker),
    });
    vi.stubGlobal("getComputedStyle", () =>
      Object.assign([], { lineHeight: "24px" }),
    );
    vi.stubGlobal(
      "DOMRect",
      class {
        constructor(
          public x: number,
          public y: number,
          public width: number,
          public height: number,
        ) {}
      },
    );
    const input = {
      value: "中文\n[[",
      offsetWidth: 300,
      scrollLeft: 2,
      scrollTop: 12,
      after: vi.fn(),
      getBoundingClientRect: () => ({ left: 100, top: 200 }),
    } as unknown as HTMLTextAreaElement;

    expect(caretRect(input, 5)).toMatchObject({
      x: 158,
      y: 228,
      width: 0,
      height: 24,
    });
    expect(mirror).toHaveProperty("textContent", "中文\n[[");
    expect(mirror.style).toHaveProperty("display", "block");
    expect(remove).toHaveBeenCalledOnce();
  });
});
