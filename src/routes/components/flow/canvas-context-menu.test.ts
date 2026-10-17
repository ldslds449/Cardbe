import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";

const source = readFileSync(
  new URL("./flow_view.svelte", import.meta.url),
  "utf8",
);
const node = parse(source, { modern: true }).instance!.content.body.find(
  (entry) =>
    entry.type === "FunctionDeclaration" &&
    entry.id?.name === "canvas_contextmenu",
)!;
const { start, end } = node as typeof node & { start: number; end: number };
class Element {
  constructor(
    private pane = false,
    private edge: string | null = null,
  ) {}
  closest() {
    return this.edge === null ? null : this;
  }
  getAttribute(name: string) {
    return name === "data-id" ? this.edge : null;
  }
  classList = {
    contains: (name: string) => this.pane && name === "svelte-flow__pane",
  };
  getBoundingClientRect() {
    return { left: 100, top: 60 };
  }
}
function canvas() {
  return new Function(
    "Element",
    "viewport",
    transpile(`
    let insertion_position = null, context_edge_id = null, selected_edges = [];
    let context_edge_snapshot = null;
    const relationship_of = (id) => (id ? { id } : null);
    ${source.slice(start, end)}
    return {
      open: canvas_contextmenu,
      position: () => insertion_position,
      edge: () => ({ context_edge_id, selected_edges }),
      snapshot: () => context_edge_snapshot,
    };
  `),
  )(Element, { x: -200, y: 80, zoom: 0.5 });
}
it("converts right-click coordinates relative to the canvas through pan and zoom", () => {
  const view = canvas();
  const preventDefault = vi.fn();
  view.open({
    target: new Element(true),
    currentTarget: new Element(),
    clientX: 300,
    clientY: 240,
    preventDefault,
  });
  expect(view.position()).toEqual({ x: 800, y: 200 });
  expect(preventDefault).not.toHaveBeenCalled();
});
it("leaves node and control context menus alone, and uses default insertion for keyboard invocation", () => {
  const view = canvas();
  const trigger = new Element();
  const preventDefault = vi.fn();
  view.open({ target: new Element(), currentTarget: trigger, preventDefault });
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(view.position()).toBeNull();
  view.open({ target: trigger, currentTarget: trigger, preventDefault });
  expect(view.position()).toBeNull();
});
it("opens relationship options on an edge and selects it", () => {
  const view = canvas();
  const preventDefault = vi.fn();
  view.open({
    target: new Element(false, "1:2"),
    currentTarget: new Element(),
    preventDefault,
  });
  expect(preventDefault).not.toHaveBeenCalled();
  expect(view.edge()).toEqual({
    context_edge_id: "1:2",
    selected_edges: ["1:2"],
  });
  // The menu keeps showing this line even if it is removed while closing.
  expect(view.snapshot()).toEqual({ id: "1:2" });
  view.open({
    target: new Element(true),
    currentTarget: new Element(),
    clientX: 0,
    clientY: 0,
    preventDefault,
  });
  expect(view.edge().context_edge_id).toBeNull();
});
