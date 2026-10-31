import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { expect, it } from "vite-plus/test";

it("mounts the reference picker with no cards in both description layouts", () => {
  const source = readFileSync(
    new URL("./task_editor.svelte", import.meta.url),
    "utf8",
  );
  const conditions: string[] = [];
  function visit(value: unknown) {
    if (!value || typeof value !== "object") {
      return;
    }
    const node = value as {
      type?: string;
      test?: { start: number; end: number };
      consequent?: { nodes: { type: string; start: number; end: number }[] };
    };
    if (
      node.type === "IfBlock" &&
      node.consequent?.nodes.some(
        (child) =>
          child.type === "RenderTag" &&
          source
            .slice(child.start, child.end)
            .includes("card_reference_picker()"),
      )
    ) {
      conditions.push(source.slice(node.test!.start, node.test!.end));
    }
    Object.values(value).forEach(visit);
  }
  visit(parse(source, { modern: true }).fragment);
  expect(conditions).toHaveLength(2);
  for (const condition of conditions) {
    const visible = new Function(
      "previewing_description",
      "description_expanded",
      "card_reference_options",
      `return ${condition}`,
    );
    expect(visible(false, false, [])).toBe(true);
    expect(visible(true, false, [])).toBe(false);
  }
});
