import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";
import {
  column_width_bounds,
  edit_dialog_width_bounds,
  LayoutWidthState,
  view_dialog_width_bounds,
} from "./layout-widths.svelte";

describe("LayoutWidthState", () => {
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

  it("keeps column widths per board and defaults elsewhere", () => {
    const state = new LayoutWidthState();
    state.set_column_width(1, 400);
    expect(state.column_width(1)).toBe(400);
    expect(state.column_width(2)).toBe(column_width_bounds.default);
    expect(state.column_width(null)).toBe(column_width_bounds.default);
    expect(new LayoutWidthState().column_width(1)).toBe(400);
  });

  it("clamps widths and ignores boardless updates", () => {
    const state = new LayoutWidthState();
    state.set_column_width(1, 10_000);
    state.set_column_width(2, 10);
    state.set_column_width(null, 500);
    state.set_view_dialog_width(Number.NaN);
    state.set_edit_dialog_width(100);
    expect(state.column_width(1)).toBe(column_width_bounds.max);
    expect(state.column_width(2)).toBe(column_width_bounds.min);
    expect(state.view_dialog_width).toBe(view_dialog_width_bounds.default);
    expect(state.edit_dialog_width).toBe(edit_dialog_width_bounds.min);
  });

  it("only persists committed changes", () => {
    const state = new LayoutWidthState();
    state.set_edit_dialog_width(800, false);
    expect(state.edit_dialog_width).toBe(800);
    expect(stored.size).toBe(0);
    state.set_edit_dialog_width(800);
    expect(new LayoutWidthState().edit_dialog_width).toBe(800);
  });

  it("drops invalid stored values", () => {
    stored.set(
      "cardbe-layout-widths",
      JSON.stringify({
        columns: { 1: 360, 2: "wide", 3: 99_999 },
        view_dialog: 700,
        edit_dialog: -1,
      }),
    );
    const state = new LayoutWidthState();
    expect(state.column_width(1)).toBe(360);
    expect(state.column_width(2)).toBe(column_width_bounds.default);
    expect(state.column_width(3)).toBe(column_width_bounds.default);
    expect(state.view_dialog_width).toBe(700);
    expect(state.edit_dialog_width).toBe(edit_dialog_width_bounds.default);

    stored.set("cardbe-layout-widths", "not json");
    expect(new LayoutWidthState().column_width(1)).toBe(
      column_width_bounds.default,
    );
  });

  it("prunes widths of deleted boards", () => {
    const state = new LayoutWidthState();
    state.set_column_width(1, 400);
    state.set_column_width(2, 480);
    state.prune_boards([2]);
    expect(state.column_width(1)).toBe(column_width_bounds.default);
    expect(new LayoutWidthState().column_width(2)).toBe(480);
  });
});
