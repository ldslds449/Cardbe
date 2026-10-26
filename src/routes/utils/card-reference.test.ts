import { describe, expect, it } from "vite-plus/test";

import {
  create_card_reference,
  render_card_references,
  task_id_from_card_reference,
} from "./card-reference";

describe("card references", () => {
  it("creates a stable reference token", () => {
    expect(create_card_reference("Fix login", "task_42")).toBe(
      "[[Fix login|task_42]]",
    );
  });

  it("renders reference tokens as internal Markdown links", () => {
    expect(render_card_references("See [[Fix login|task_42]] next.")).toBe(
      "See [Fix login](#card-task_42) next.",
    );
  });

  it("preserves title brackets while escaping the preview link label", () => {
    const reference = create_card_reference("[待辦] 修正 ]] 問題", "task_42");
    expect(reference).toBe("[[[待辦] 修正 ]] 問題|task_42]]");
    expect(render_card_references(reference)).toBe(
      "[\\[待辦\\] 修正 \\]\\] 問題](#card-task_42)",
    );
    expect(render_card_references(`${reference} [[Next|task_43]]`)).toBe(
      "[\\[待辦\\] 修正 \\]\\] 問題](#card-task_42) [Next](#card-task_43)",
    );
  });

  it("recognizes only card-reference hrefs", () => {
    expect(task_id_from_card_reference("#card-task_42")).toBe("task_42");
    expect(task_id_from_card_reference("https://example.com")).toBeUndefined();
  });
});
