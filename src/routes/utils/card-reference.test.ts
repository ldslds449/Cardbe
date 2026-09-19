import { describe, expect, it } from "vitest";

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

  it("recognizes only card-reference hrefs", () => {
    expect(task_id_from_card_reference("#card-task_42")).toBe("task_42");
    expect(task_id_from_card_reference("https://example.com")).toBeUndefined();
  });
});
