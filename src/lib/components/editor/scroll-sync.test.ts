import { describe, expect, it } from "vite-plus/test";

import { invert_anchors, map_scroll, scroll_anchors } from "./scroll-sync";

describe("scroll sync", () => {
  it("frames matching positions with both scroll ranges", () => {
    expect(
      scroll_anchors(
        [
          [100, 300],
          [400, 500],
        ],
        1000,
        2000,
      ),
    ).toEqual([
      [0, 0],
      [100, 300],
      [400, 500],
      [1000, 2000],
    ]);
  });

  it("drops anchors that would go backwards or past the end", () => {
    expect(
      scroll_anchors(
        [
          [100, 300],
          [200, 250],
          [150, 400],
          [1200, 1500],
        ],
        1000,
        2000,
      ),
    ).toEqual([
      [0, 0],
      [100, 300],
      [150, 400],
      [1000, 2000],
    ]);
  });

  it("interpolates between anchors and clamps outside them", () => {
    const anchors = scroll_anchors([[100, 300]], 1100, 1300);
    expect(map_scroll(anchors, 0)).toBe(0);
    expect(map_scroll(anchors, 50)).toBe(150);
    expect(map_scroll(anchors, 600)).toBe(800);
    expect(map_scroll(anchors, 5000)).toBe(1300);
    expect(map_scroll(invert_anchors(anchors), 150)).toBe(50);
  });

  it("handles content that does not scroll", () => {
    const anchors = scroll_anchors([], 0, 0);
    expect(map_scroll(anchors, 0)).toBe(0);
    expect(map_scroll(scroll_anchors([], 0, 500), 0)).toBe(0);
  });
});
