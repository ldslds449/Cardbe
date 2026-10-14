import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";
import { added_layout, moved_layout, type FlowLayout } from "./flow";

const source = readFileSync(
  new URL("./flow_view.svelte", import.meta.url),
  "utf8",
);
const functions = parse(source, { modern: true })
  .instance!.content.body.filter(
    (node) =>
      node.type === "FunctionDeclaration" &&
      ["add_nodes", "add_notes"].includes(node.id!.name),
  )
  .map((node) => {
    const { start, end } = node as typeof node & { start: number; end: number };
    return source.slice(start, end);
  })
  .join("\n");

function picker(saved: boolean) {
  const save_layout = vi.fn(async (_layout: FlowLayout) => saved);
  let next_id = 0;
  const code = transpile(`
    let read_only = false, saving = false, loaded = true;
    const data = { layout: { nodes: [], viewport: null, annotations: [{ id: "old", kind: "note", note_id: 5, x: 0, y: 0, width: 280 }] } };
    const nodes = [], viewport = { x: 0, y: 0, zoom: 1 }, insertion_position = { x: 100, y: 200 };
    const columns = [{ tasks: [{ id: "task_1" }] }], selected_notes = [{ id: 1 }, { id: 2 }];
    let selected_tasks = ["task_1"], selected_task_columns = ["column_1"], selected_note_ids = [1, 2];
    let add_open = true, note_picker_open = true;
    ${functions}
    return { add_nodes, add_notes, selection: () => ({ selected_tasks, selected_task_columns, selected_note_ids, add_open, note_picker_open }) };
  `);
  const actions = new Function(
    "save_layout",
    "moved_layout",
    "added_layout",
    "new_annotation_position",
    code,
  )(save_layout, moved_layout, added_layout, () => ({
    id: `new-${++next_id}`,
    x: 100,
    y: 200,
    width: 280,
  }));
  return { ...actions, save_layout };
}

it.each([false, true])(
  "only clears selections and closes pickers after a successful save (%s)",
  async (saved) => {
    const view = picker(saved);
    await view.add_nodes(["task_1"]);
    await view.add_notes();
    expect(view.selection()).toEqual({
      selected_tasks: saved ? [] : ["task_1"],
      selected_task_columns: saved ? [] : ["column_1"],
      selected_note_ids: saved ? [] : [1, 2],
      add_open: !saved,
      note_picker_open: !saved,
    });
  },
);

it("saves the selected notes together, preserves existing notes, and places new notes apart", async () => {
  const view = picker(true);
  await view.add_notes();
  expect(view.save_layout).toHaveBeenCalledOnce();
  expect(view.save_layout.mock.calls[0][0].annotations).toEqual([
    { id: "old", kind: "note", note_id: 5, x: 0, y: 0, width: 280 },
    { id: "new-1", kind: "note", note_id: 1, x: 100, y: 200, width: 280 },
    { id: "new-2", kind: "note", note_id: 2, x: 420, y: 200, width: 280 },
  ]);
});
