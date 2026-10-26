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
const declaration = parse(source, { modern: true }).instance!.content.body.find(
  (node) => node.type === "FunctionDeclaration" && node.id?.name === "save",
)!;
const node = declaration as typeof declaration & { start: number; end: number };

function plan() {
  const previous = [{ board_id: 1, task: { id: "task_1" } }];
  const invoke = vi.fn();
  const refresh = vi.fn();
  const toast = { error: vi.fn() };
  const scope = {
    invoke,
    refresh,
    toast,
    get_task_id: (id: string) => Number(id.slice(5)),
    translateCommandError: () => "failed",
    previous,
  };
  const state = new Function(
    ...Object.keys(scope),
    transpile(`
    let busy = false, loaded = true, items = previous, plan_request = 0;
    const MAX_TASKS = 100;
    ${source.slice(node.start, node.end)}
    return { save, get items() { return items; }, get busy() { return busy; } };
  `),
  )(...Object.values(scope));
  return { state, previous, invoke, refresh, toast };
}

it("updates saved selection without reloading the list and blocks duplicate writes", async () => {
  const { state, previous, invoke, refresh } = plan();
  let finish!: () => void;
  invoke.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  const next = [...previous, { board_id: 2, task: { id: "task_1" } }];
  const saving = state.save(next);
  expect(state.busy).toBe(true);
  expect(state.items).toBe(previous);
  await state.save(next);
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(invoke).toHaveBeenCalledWith("set_daily_plan", {
    tasks: [
      { board_id: 1, task_id: 1 },
      { board_id: 2, task_id: 1 },
    ],
  });
  finish();
  await saving;
  expect(state.items).toBe(next);
  expect(state.busy).toBe(false);
  expect(refresh).not.toHaveBeenCalled();
});

it("keeps the previous selection and reports failed writes without reloading", async () => {
  const { state, previous, invoke, refresh, toast } = plan();
  invoke.mockRejectedValue(new Error("offline"));
  await state.save([]);
  expect(state.items).toBe(previous);
  expect(state.busy).toBe(false);
  expect(toast.error).toHaveBeenCalledWith("failed");
  expect(refresh).not.toHaveBeenCalled();
});
