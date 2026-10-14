import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";

const source = readFileSync(
  new URL("./annotation_node.svelte", import.meta.url),
  "utf8",
);
const statements = parse(source, { modern: true }).instance!.content.body;
const code = statements
  .filter(
    (node) =>
      (node.type === "FunctionDeclaration" && node.id?.name === "commit") ||
      (node.type === "ExpressionStatement" &&
        node.expression.type === "CallExpression" &&
        node.expression.callee.type === "Identifier" &&
        node.expression.callee.name === "onMount" &&
        source
          .slice(
            (node as unknown as { start: number }).start,
            (node as unknown as { end: number }).end,
          )
          .includes("outside_pointerdown")),
  )
  .map((node) => {
    const { start, end } = node as typeof node & { start: number; end: number };
    return source.slice(start, end);
  })
  .join("\n");

function setup(read_only = false) {
  const window = new EventTarget();
  const frame = {};
  const data = {
    annotation: { id: "text-1" },
    read_only,
    onSaveText: vi.fn(async () => true),
  };
  let cleanup!: () => void;
  const register = vi.spyOn(window, "addEventListener");
  const editor = new Function(
    "window",
    "onMount",
    "data",
    "frame",
    transpile(`
    let editing = true, committing = false, pending_commit = false;
    const draft = "Updated text";
    ${code}
    return { commit, pending: () => pending_commit };
  `),
  )(
    window,
    (callback: () => () => void) => {
      cleanup = callback();
    },
    data,
    frame,
  );
  function pointerdown(inside = false) {
    const event = new Event("pointerdown", { cancelable: true });
    event.composedPath = () => (inside ? [frame as EventTarget] : []);
    // Canvas interactions can prevent the default focus change.
    event.preventDefault();
    window.dispatchEvent(event);
  }
  return { data, editor, pointerdown, cleanup, register };
}

it("commits external pointer presses without requiring blur, ignores internal presses, and cleans up", async () => {
  const { data, pointerdown, cleanup, register } = setup();
  expect(register).toHaveBeenCalledWith("pointerdown", expect.any(Function), {
    capture: true,
  });
  pointerdown(true);
  expect(data.onSaveText).not.toHaveBeenCalled();
  pointerdown();
  pointerdown();
  expect(data.onSaveText).toHaveBeenCalledExactlyOnceWith(
    "text-1",
    "Updated text",
  );
  await Promise.resolve();
  cleanup();
  pointerdown();
  expect(data.onSaveText).toHaveBeenCalledOnce();
});

it("retains a commit requested during another save until editing becomes writable", async () => {
  const { data, editor, pointerdown, cleanup } = setup(true);
  pointerdown();
  expect(editor.pending()).toBe(true);
  expect(data.onSaveText).not.toHaveBeenCalled();
  data.read_only = false;
  await editor.commit();
  expect(editor.pending()).toBe(false);
  expect(data.onSaveText).toHaveBeenCalledExactlyOnceWith(
    "text-1",
    "Updated text",
  );
  cleanup();
});
