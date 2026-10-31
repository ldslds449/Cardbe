import { describe, expect, it } from "vite-plus/test";
import { createRawSnippet } from "svelte";
import { render } from "svelte/server";
import { m } from "#lib/paraglide/messages.js";
import ZoomableView, { nextZoom } from "./zoomable-view.svelte";

describe("zoomable view", () => {
  it("steps through zoom levels and stops at the bounds", () => {
    expect(nextZoom(1, 1)).toBe(1.25);
    expect(nextZoom(1, -1)).toBe(0.75);
    expect(nextZoom(4, 1)).toBe(6);
    expect(nextZoom(10, 1)).toBe(10);
    expect(nextZoom(0.25, -1)).toBe(0.25);
  });

  it("renders zoom and full-screen controls around its content", () => {
    const { body } = render(ZoomableView, {
      props: {
        label: "Diagram",
        children: createRawSnippet(() => ({ render: () => "<svg></svg>" })),
      },
    });
    for (const label of [
      m.markdown_zoom_out(),
      m.markdown_zoom_reset(),
      m.markdown_zoom_in(),
      m.markdown_view_full(),
    ]) {
      expect(body).toContain(`aria-label="${label}"`);
    }
    expect(body).toContain("<svg></svg>");
  });
});
