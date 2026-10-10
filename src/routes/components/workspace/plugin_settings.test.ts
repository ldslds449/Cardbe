import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";

const source = readFileSync(
  new URL("./plugin_settings.svelte", import.meta.url),
  "utf8",
);
const functions = parse(source, { modern: true })
  .instance!.content.body.filter(
    (node) =>
      node.type === "FunctionDeclaration" &&
      ["chooseBoard", "refreshColumns"].includes(node.id!.name),
  )
  .map((node) => {
    const { start, end } = node as typeof node & { start: number; end: number };
    return source.slice(start, end);
  })
  .join("\n");

function setup(invoke: ReturnType<typeof vi.fn>) {
  return new Function(
    "invoke",
    "parseCommandError",
    transpile(`
      let boardId = "1", columnRequest = 0, columns = [], error = null;
      const editing = { settings: [{ key: "column", type: "column" }] };
      const config = { column: 7 };
      ${functions}
      return { chooseBoard, refreshColumns,
        state: () => ({ columns, config, error }) };
    `),
  )(invoke, (error: unknown) => error);
}

it("fetches newly added columns without clearing the selected column", async () => {
  const invoke = vi.fn().mockResolvedValueOnce([]);
  const settings = setup(invoke);
  await settings.refreshColumns();
  expect(settings.state().columns).toEqual([]);
  invoke.mockResolvedValueOnce([{ id: 7, name: "New column" }]);
  await settings.refreshColumns();
  expect(invoke).toHaveBeenLastCalledWith("get_board_columns", { boardId: 1 });
  expect(settings.state().columns).toEqual([{ id: 7, name: "New column" }]);
  expect(settings.state().config.column).toBe(7);
});

it("ignores a stale refresh after switching boards", async () => {
  let resolve!: (columns: unknown[]) => void;
  const invoke = vi
    .fn()
    .mockReturnValueOnce(new Promise((done) => (resolve = done)))
    .mockResolvedValueOnce([{ id: 8, name: "Other board" }]);
  const settings = setup(invoke);
  const stale = settings.refreshColumns();
  await settings.chooseBoard("2");
  resolve([{ id: 7, name: "Old board" }]);
  await stale;
  expect(settings.state().columns).toEqual([{ id: 8, name: "Other board" }]);
  expect(settings.state().config.column).toBeUndefined();
});

it("keeps existing options and reports structured refresh failures", async () => {
  const invoke = vi.fn().mockResolvedValueOnce([{ id: 7, name: "Column" }]);
  const settings = setup(invoke);
  await settings.refreshColumns();
  invoke.mockRejectedValueOnce({ code: "BOARD_NOT_FOUND" });
  await settings.refreshColumns();
  expect(settings.state().columns).toEqual([{ id: 7, name: "Column" }]);
  expect(settings.state().error).toEqual({ code: "BOARD_NOT_FOUND" });
});
