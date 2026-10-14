import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";

const source = readFileSync(new URL("./+page.svelte", import.meta.url), "utf8");
const names = new Set([
  "open_add_task",
  "open_add_task_shortcut",
  "open_add_task_for_date",
  "handle_task_column_select",
]);
const functions = parse(source, { modern: true })
  .instance!.content.body.filter(
    (node) => node.type === "FunctionDeclaration" && names.has(node.id!.name),
  )
  .map((node) => {
    const { start, end } = node as typeof node & { start: number; end: number };
    return source.slice(start, end);
  })
  .join("\n");

function creation(columns = [{ id: "column_1" }]) {
  const scope = {
    board: { columns },
    active_board_read_only: false,
    add_task_routine: vi.fn(),
    open_add_column_dialog: vi.fn(),
    open_imported_task: vi.fn(),
  };
  const code = `let pending_import_task = null, pending_task_due_time, pending_task_created;
    let task_column_id = '', task_column_error = false, task_column_dialog_open = false;
    ${functions}
    return { open_add_task, open_add_task_shortcut, open_add_task_for_date,
      select(id) { task_column_id = id; handle_task_column_select(); },
      get open() { return task_column_dialog_open; } };`;
  const actions = new Function(...Object.keys(scope), transpile(code))(
    ...Object.values(scope),
  );
  return { ...scope, ...actions };
}

it("uses the same column selection for the shortcut and canvas, forwarding creation only after selection", () => {
  const page = creation();
  const added = vi.fn();
  page.open_add_task(added);
  expect(page.add_task_routine).not.toHaveBeenCalled();
  page.select("missing");
  expect(page.add_task_routine).not.toHaveBeenCalled();
  page.select("column_1");
  expect(page.add_task_routine).toHaveBeenLastCalledWith(0, undefined, added);
  page.open_add_task_shortcut();
  page.select("column_1");
  expect(page.add_task_routine).toHaveBeenLastCalledWith(
    0,
    undefined,
    undefined,
  );
  expect(added).not.toHaveBeenCalled();
});

it("clears a cancelled canvas callback when creating from the calendar", () => {
  const page = creation([{ id: "column_1" }, { id: "column_2" }]);
  page.open_add_task(vi.fn());
  const date = new Date("2026-10-12T12:00:00Z");
  page.open_add_task_for_date(date);
  page.select("column_2");
  expect(page.add_task_routine).toHaveBeenLastCalledWith(1, date, undefined);
});

it("opens column creation on an empty board for both entry points", () => {
  const page = creation([]);
  page.open_add_task(vi.fn());
  page.open_add_task_shortcut();
  expect(page.open_add_column_dialog).toHaveBeenCalledTimes(2);
  expect(page.add_task_routine).not.toHaveBeenCalled();
});
