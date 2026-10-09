import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";

const source = readFileSync(
  new URL("./color-picker.svelte", import.meta.url),
  "utf8",
);
const statements = parse(source, {
  modern: true,
}).instance!.content.body.filter(
  (node) =>
    (node.type === "VariableDeclaration" &&
      node.declarations.some(
        (declaration) =>
          declaration.id.type === "Identifier" &&
          ["isDragging", "stopDrag"].includes(declaration.id.name),
      )) ||
    (node.type === "FunctionDeclaration" &&
      node.id?.name === "handleDragStart") ||
    (node.type === "ExpressionStatement" &&
      node.expression.type === "CallExpression" &&
      node.expression.callee.type === "Identifier" &&
      node.expression.callee.name === "onDestroy"),
);
const code = transpile(
  statements
    .map((node) => {
      // The parser returns offsets that its ESTree types omit.
      const { start, end } = node as typeof node & {
        start: number;
        end: number;
      };
      return source.slice(start, end);
    })
    .join("\n") + "\nreturn handleDragStart;",
);

function picker() {
  const window = new EventTarget();
  let destroy!: () => void;
  const start = new Function("window", "onDestroy", "$state", code)(
    window,
    (callback: () => void) => {
      destroy = callback;
    },
    (value: unknown) => value,
  );
  return { window, start, destroy };
}

it("removes drag listeners when the picker is unmounted", () => {
  const { window, start, destroy } = picker();
  const update = vi.fn();
  start(new Event("mousedown"), update);
  window.dispatchEvent(new Event("mousemove"));
  destroy();
  window.dispatchEvent(new Event("mousemove"));
  expect(update).toHaveBeenCalledTimes(2);
});

it.each(["blur", "touchcancel"])("stops dragging on %s", (event) => {
  const { window, start } = picker();
  const update = vi.fn();
  start(new Event("mousedown"), update);
  window.dispatchEvent(new Event(event));
  window.dispatchEvent(new Event("mousemove"));
  expect(update).toHaveBeenCalledOnce();
});

it("replaces the previous drag listeners", () => {
  const { window, start, destroy } = picker();
  const previous = vi.fn();
  const current = vi.fn();
  start(new Event("mousedown"), previous);
  start(new Event("mousedown"), current);
  window.dispatchEvent(new Event("mousemove"));
  expect(previous).toHaveBeenCalledOnce();
  expect(current).toHaveBeenCalledTimes(2);
  destroy();
});
