import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";

const source = readFileSync(
  new URL("./daily_plan.svelte", import.meta.url),
  "utf8",
)
  .replace(/^\uFEFF/, "")
  .replaceAll("\r\n", "\n");
const functions = parse(source, { modern: true })
  .instance!.content.body.filter(
    (node) =>
      node.type === "FunctionDeclaration" &&
      ["drag_start", "drag_over", "drag_end"].includes(node.id!.name),
  )
  .map((node) => {
    const position = node as typeof node & { start: number; end: number };
    return source.slice(position.start, position.end);
  })
  .join("\n");
const items = [
  { board_id: 1, task: { id: "task_1" } },
  { board_id: 2, task: { id: "task_1" } },
  { board_id: 1, task: { id: "task_2" } },
];
function dragging() {
  const save = vi.fn().mockResolvedValue(undefined);
  const actions = new Function(
    "initial",
    "save",
    transpile(`
    let items = initial, drag_items = null, drag_base = null, busy = false;
    const key = item => item.board_id + ':' + item.task.id;
    ${functions}
    return { drag_start, drag_over, drag_end, get preview() { return drag_items; }, replace() { items = [...items]; } };
  `),
  )(items, save);
  return { actions, save };
}
const event = {
  canceled: false,
  operation: {
    source: { id: "1:task_1", type: "daily-plan-task" },
    target: { id: "2:task_1", type: "daily-plan-task" },
  },
};

it("previews composite task identities and saves once on drop without snapping back while saving", async () => {
  const { actions, save } = dragging();
  let finish!: () => void;
  save.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  actions.drag_start();
  actions.drag_over(event);
  const next = [items[1], items[0], items[2]];
  expect(actions.preview).toEqual(next);
  expect(save).not.toHaveBeenCalled();
  const pending = actions.drag_end(event);
  expect(actions.preview).toEqual(next);
  expect(save).toHaveBeenCalledExactlyOnceWith(next);
  finish();
  await pending;
  expect(actions.preview).toBeNull();
});

it("does not save cancelled, outside, unchanged or stale drops", async () => {
  for (const mode of ["cancelled", "outside", "unchanged", "stale"]) {
    const { actions, save } = dragging();
    actions.drag_start();
    if (mode !== "unchanged") {
      actions.drag_over(event);
    }
    if (mode === "stale") {
      actions.replace();
    }
    await actions.drag_end({
      ...event,
      canceled: mode === "cancelled",
      operation: {
        ...event.operation,
        target: mode === "outside" ? null : event.operation.target,
      },
    });
    expect(save).not.toHaveBeenCalled();
    expect(actions.preview).toBeNull();
  }
});
