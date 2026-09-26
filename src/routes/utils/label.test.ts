import { describe, expect, it } from "vitest";

import { parse_label } from "./label";

describe("parse_label", () => {
  it("parses reserved namespaces case-insensitively", () => {
    expect(parse_label("owner:Alice")).toEqual({
      kind: "owner",
      value: "Alice",
    });
    expect(parse_label(" TYPE: Bug ")).toEqual({
      kind: "category",
      value: "Bug",
    });
    expect(parse_label("PRIORITY: High")).toEqual({
      kind: "priority",
      value: "High",
    });
    expect(parse_label("status:blocked")).toEqual({
      kind: "status",
      value: "blocked",
    });
    expect(parse_label("effort:large")).toEqual({
      kind: "effort",
      value: "large",
    });
  });

  it("preserves ordinary and unknown labels", () => {
    expect(parse_label("urgent")).toEqual({ kind: "general", value: "urgent" });
    expect(parse_label("severity:high")).toEqual({
      kind: "general",
      value: "severity:high",
    });
  });

  it("does not hide an incomplete reserved label", () => {
    expect(parse_label("owner:")).toEqual({ kind: "general", value: "owner:" });
    expect(parse_label("type:   ")).toEqual({
      kind: "general",
      value: "type:   ",
    });
    expect(parse_label("priority:   ")).toEqual({
      kind: "general",
      value: "priority:   ",
    });
  });
});
