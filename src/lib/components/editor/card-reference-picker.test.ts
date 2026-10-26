import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";
import { create_card_reference } from "../../../routes/utils/card-reference";

const source = readFileSync(
  new URL("./card-reference-picker.svelte", import.meta.url),
  "utf8",
);

it("binds rich suggestion visibility so closing one picker does not suppress the next", () => {
  const richSource = readFileSync(
    new URL("./rich-text-surface.svelte", import.meta.url),
    "utf8",
  );
  expect(richSource).toContain("bind:open={referenceOpen}");
  const show = parse(richSource, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" && node.id?.name === "showReference",
  )! as unknown as { start: number; end: number };
  const picker = new Function(
    transpile(`
    let referenceOpen = false, reference = null;
    ${richSource.slice(show.start, show.end)}
    return { showReference, close() { referenceOpen = false; }, isOpen() { return referenceOpen; } };
  `),
  )();
  for (const from of [1, 5, 9]) {
    picker.showReference({ range: { from, to: from + 2 }, query: "" });
    expect(picker.isOpen()).toBe(true);
    picker.close();
    expect(picker.isOpen()).toBe(false);
  }
});
const node = parse(source, { modern: true }).instance!.content.body.find(
  (node) =>
    node.type === "FunctionDeclaration" &&
    node.id?.name === "insert_card_reference",
)! as unknown as { start: number; end: number };

it.each([0, 1])(
  "replaces the trigger in either editor coordinate system: %s",
  async (start) => {
    const editor = {
      getSelection: () => ({ from: start + 2, to: start + 2 }),
      replaceMarkdown: vi.fn(),
      focus: vi.fn(),
    };
    const onclose = vi.fn();
    const insert = new Function(
      "editor",
      "onclose",
      "tick",
      "create_card_reference",
      "start",
      transpile(`let open=true, triggerStart=start;
      ${source.slice(node.start, node.end)}
      return insert_card_reference;`),
    )(editor, onclose, async () => {}, create_card_reference, start);
    await insert({ task: { title: "Card", id: "task_123" } });
    expect(editor.replaceMarkdown).toHaveBeenCalledWith("[[Card|task_123]]", {
      from: start,
      to: start + 2,
    });
    expect(onclose).toHaveBeenCalledOnce();
    expect(editor.focus).toHaveBeenCalledOnce();
  },
);
