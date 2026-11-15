import { describe, expect, it } from "vite-plus/test";
import { has_primary_modifier } from "./platform";

describe("has_primary_modifier", () => {
  it("uses Command on Apple platforms", () => {
    expect(has_primary_modifier({ ctrlKey: false, metaKey: true }, true)).toBe(
      true,
    );
    expect(has_primary_modifier({ ctrlKey: true, metaKey: false }, true)).toBe(
      false,
    );
    expect(has_primary_modifier({ ctrlKey: true, metaKey: true }, true)).toBe(
      false,
    );
  });

  it("uses Ctrl elsewhere", () => {
    expect(has_primary_modifier({ ctrlKey: true, metaKey: false }, false)).toBe(
      true,
    );
    expect(has_primary_modifier({ ctrlKey: false, metaKey: true }, false)).toBe(
      false,
    );
  });
});
