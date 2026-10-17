import { describe, expect, it } from "vite-plus/test";
import {
  display_shortcut,
  held_modifiers,
  shortcut_from_keydown,
} from "./global-shortcuts";

const keydown = (code: string, modifiers: string[] = []) => ({
  code,
  ctrlKey: modifiers.includes("ctrl"),
  altKey: modifiers.includes("alt"),
  shiftKey: modifiers.includes("shift"),
  metaKey: modifiers.includes("meta"),
});

describe("global shortcut recording", () => {
  it("builds canonical shortcuts from physical keys", () => {
    expect(
      shortcut_from_keydown(keydown("KeyF", ["shift", "alt", "ctrl"])),
    ).toEqual({ kind: "shortcut", shortcut: "Ctrl+Alt+Shift+F" });
    expect(shortcut_from_keydown(keydown("Digit7", ["alt"]))).toEqual({
      kind: "shortcut",
      shortcut: "Alt+7",
    });
    expect(shortcut_from_keydown(keydown("F24", ["ctrl"]))).toEqual({
      kind: "shortcut",
      shortcut: "Ctrl+F24",
    });
    expect(display_shortcut("Ctrl+Alt+T")).toBe("Ctrl + Alt + T");
  });

  it("waits while only modifiers are held", () => {
    expect(shortcut_from_keydown(keydown("ControlLeft", ["ctrl"]))).toEqual({
      kind: "incomplete",
    });
    expect(shortcut_from_keydown(keydown("AltRight", ["alt"]))).toEqual({
      kind: "incomplete",
    });
    expect(held_modifiers(keydown("ShiftLeft", ["shift", "ctrl"]))).toEqual([
      "Ctrl",
      "Shift",
    ]);
    expect(held_modifiers(keydown("MetaLeft", ["meta"]))).toEqual([]);
  });

  it("rejects combinations the backend would reject", () => {
    for (const event of [
      keydown("KeyF"),
      keydown("KeyF", ["shift"]),
      keydown("KeyF", ["ctrl", "meta"]),
      keydown("Space", ["ctrl"]),
      keydown("F25", ["ctrl"]),
    ]) {
      expect(shortcut_from_keydown(event)).toEqual({ kind: "modifier" });
    }
  });
});
