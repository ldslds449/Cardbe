import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";
import { TaskExpansionState } from "./task-expansion.svelte";

describe("TaskExpansionState", () => {
  let stored: Map<string, string>;

  beforeEach(() => {
    stored = new Map();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => stored.set(key, value),
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("toggles tasks per board without cross-board collisions", () => {
    const state = new TaskExpansionState();
    state.toggle(1, "7");
    expect(state.is_expanded(1, "7")).toBe(true);
    expect(state.is_expanded(2, "7")).toBe(false);
    expect(state.is_expanded(null, "7")).toBe(false);
    state.toggle(1, "7");
    expect(state.is_expanded(1, "7")).toBe(false);
  });

  it("restores persisted expansion and ignores malformed storage", () => {
    new TaskExpansionState().toggle(3, "1");
    expect(new TaskExpansionState().is_expanded(3, "1")).toBe(true);

    stored.set("cardbe-task-expansion", "not json");
    expect(new TaskExpansionState().is_expanded(3, "1")).toBe(false);
    stored.set("cardbe-task-expansion", '{"3":[1,"2"],"4":"x"}');
    const restored = new TaskExpansionState();
    expect(restored.is_expanded(3, "2")).toBe(true);
    expect(restored.is_expanded(3, "1")).toBe(false);
  });

  it("prunes missing tasks and clears one board", () => {
    const state = new TaskExpansionState();
    state.toggle(1, "1");
    state.toggle(1, "2");
    state.toggle(2, "1");
    state.prune(1, ["2"]);
    expect(state.is_expanded(1, "1")).toBe(false);
    expect(state.is_expanded(1, "2")).toBe(true);
    expect(state.is_expanded(2, "1")).toBe(true);

    state.clear(1);
    expect(state.is_expanded(1, "2")).toBe(false);
    expect(state.is_expanded(2, "1")).toBe(true);
    expect(new TaskExpansionState().is_expanded(1, "2")).toBe(false);
    expect(new TaskExpansionState().is_expanded(2, "1")).toBe(true);
  });

  it("works when storage is unavailable", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    const state = new TaskExpansionState();
    state.toggle(1, "1");
    expect(state.is_expanded(1, "1")).toBe(true);
  });
});
