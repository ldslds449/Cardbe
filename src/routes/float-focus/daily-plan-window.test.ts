import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";
import { create_focus } from "../type/focus";
import { DAILY_PLAN_ID } from "#lib/floating-focuses.js";

const source = readFileSync(new URL("./+page.svelte", import.meta.url), "utf8")
  .replace(/^\uFEFF/, "")
  .replaceAll("\r\n", "\n");
const declaration = parse(source, { modern: true }).instance!.content.body.find(
  (node) => node.type === "FunctionDeclaration" && node.id?.name === "load",
)!;
const position = declaration as typeof declaration & {
  start: number;
  end: number;
};

function page(daily_plan: boolean) {
  const invoke = vi.fn().mockResolvedValue({
    boards: [{ id: 1, name: "Board", shared_role: "owner" }],
  });
  const close_floating_window = vi.fn();
  const load_focus_results = vi.fn();
  const scope = {
    daily_plan,
    view_id: daily_plan ? DAILY_PLAN_ID : "deleted",
    DAILY_PLAN_ID,
    create_focus,
    invoke,
    close_floating_window,
    load_focus_results,
    m: { daily_plan_title: () => "Today" },
    logger: { warn: vi.fn() },
    parseCommandError: (error: unknown) => error,
  };
  const actions = new Function(
    ...Object.keys(scope),
    transpile(`
    let load_generation = 0, loaded_snapshot = '', loading = false;
    let view = null, boards = [], items = [], has_more = false, now, error = null;
    const PAGE_SIZE = 100;
    ${source.slice(position.start, position.end)}
    return { load, get view() { return view; }, get boards() { return boards; } };
  `),
  )(...Object.values(scope));
  return { actions, invoke, close_floating_window, load_focus_results };
}

it("opens the built-in plan without requiring a saved Focus or running a Focus query", async () => {
  const { actions, invoke, close_floating_window, load_focus_results } =
    page(true);
  await actions.load();
  expect(actions.view.id).toBe(DAILY_PLAN_ID);
  expect(actions.boards).toEqual([
    { id: 1, name: "Board", shared_role: "owner" },
  ]);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("get_boards");
  expect(load_focus_results).not.toHaveBeenCalled();
  expect(close_floating_window).not.toHaveBeenCalled();
});

it("still closes a deleted user Focus", async () => {
  const { actions, invoke, close_floating_window } = page(false);
  invoke.mockImplementation(async (command: string) =>
    command === "get_focuses" ? [] : { boards: [] },
  );
  await actions.load();
  expect(close_floating_window).toHaveBeenCalledOnce();
});
