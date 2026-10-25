import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), warn: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("#lib/logger.js", () => ({ logger: { warn: mocks.warn } }));

import {
  decode_floating_focus_id,
  forget_floating_focus,
  open_floating_focus,
  read_floating_focuses,
  restore_floating_focuses,
  resize_floating_focus,
  read_floating_focus_state,
  write_floating_focus_state,
} from "./floating-focuses";
import { LogicalSize } from "@tauri-apps/api/dpi";

describe("floating Focus windows", () => {
  it("changes only height when collapsing and expanding", async () => {
    let width = 400;
    let height = 480;
    let maxHeight: number | undefined = 40;
    const float_window = {
      setResizable: vi.fn().mockResolvedValue(undefined),
      setSizeConstraints: vi.fn(async (constraints: { maxHeight?: number }) => {
        maxHeight = constraints.maxHeight;
      }),
      setSize: vi.fn(async (size: LogicalSize) => {
        width = size.width;
        height = Math.min(size.height, maxHeight ?? Infinity);
      }),
    };
    const original = new LogicalSize(400, 480);
    await resize_floating_focus(float_window, true, original);
    expect(height).toBe(42);
    expect(width).toBe(400);
    expect(float_window.setResizable).toHaveBeenLastCalledWith(false);
    expect(float_window.setResizable.mock.invocationCallOrder[0]).toBeLessThan(
      float_window.setSize.mock.invocationCallOrder[0],
    );
    expect(float_window.setSizeConstraints).toHaveBeenLastCalledWith({
      minWidth: 220,
      minHeight: 42,
      maxHeight: 42,
    });
    await resize_floating_focus(float_window, false, original);
    expect(height).toBe(480);
    expect(float_window.setResizable).toHaveBeenLastCalledWith(true);
    expect(
      float_window.setResizable.mock.invocationCallOrder[2],
    ).toBeGreaterThan(float_window.setSize.mock.invocationCallOrder[1]);
    expect(width).toBe(400);
    expect(float_window.setSizeConstraints).toHaveBeenLastCalledWith({
      minWidth: 220,
      minHeight: 120,
    });
    expect(original).toEqual(new LogicalSize(400, 480));
    expect(float_window.setSize).toHaveBeenLastCalledWith(original);
    expect(
      float_window.setSizeConstraints.mock.invocationCallOrder[1],
    ).toBeLessThan(float_window.setSize.mock.invocationCallOrder[1]);
  });
  it("does not widen a narrow window when collapsing", async () => {
    const float_window = {
      setResizable: vi.fn().mockResolvedValue(undefined),
      setSizeConstraints: vi.fn().mockResolvedValue(undefined),
      setSize: vi.fn().mockResolvedValue(undefined),
    };
    await resize_floating_focus(float_window, true, new LogicalSize(220, 360));
    expect(float_window.setSize).toHaveBeenCalledWith(new LogicalSize(220, 42));
  });
  let storage: Map<string, string>;

  beforeEach(() => {
    vi.clearAllMocks();
    storage = new Map();
    vi.stubGlobal("window", {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
        removeItem: (key: string) => storage.delete(key),
      },
    });
    mocks.invoke.mockResolvedValue(undefined);
  });
  afterEach(() => vi.unstubAllGlobals());

  it("persists each Focus's collapsed state and expanded height across restore", async () => {
    expect(read_floating_focus_state("a")).toEqual({
      collapsed: false,
      expanded_height: 360,
    });
    await open_floating_focus("a");
    write_floating_focus_state("a", true, 540);
    await restore_floating_focuses();
    expect(read_floating_focus_state("a")).toEqual({
      collapsed: true,
      expanded_height: 540,
    });
    expect(read_floating_focus_state("b").collapsed).toBe(false);
    write_floating_focus_state("a", false, 540);
    expect(read_floating_focus_state("a")).toEqual({
      collapsed: false,
      expanded_height: 540,
    });
    forget_floating_focus("a");
    expect(read_floating_focus_state("a")).toEqual({
      collapsed: false,
      expanded_height: 360,
    });
  });

  it("ignores malformed Focus preferences and handles unavailable storage", () => {
    for (const value of [
      "broken",
      "null",
      "[]",
      '{"collapsed":"true","expanded_height":360}',
      '{"collapsed":true,"expanded_height":42}',
    ]) {
      storage.set("cardbe.floating-focus-state.a", value);
      expect(read_floating_focus_state("a")).toEqual({
        collapsed: false,
        expanded_height: 360,
      });
    }
    vi.stubGlobal("window", {
      get localStorage() {
        throw new Error("unavailable");
      },
    });
    expect(read_floating_focus_state("a").collapsed).toBe(false);
    expect(() => write_floating_focus_state("a", true, 360)).not.toThrow();
    expect(mocks.warn).toHaveBeenCalledOnce();
  });

  it("remembers each opened view once and forgets closed ones", async () => {
    await open_floating_focus("a");
    await open_floating_focus("a");
    await open_floating_focus("b");
    expect(mocks.invoke).toHaveBeenCalledWith("open_floating_focus", {
      id: "a",
    });
    expect(read_floating_focuses()).toEqual(["a", "b"]);
    forget_floating_focus("a");
    expect(read_floating_focuses()).toEqual(["b"]);
  });

  it("does not remember a window that failed to open", async () => {
    mocks.invoke.mockRejectedValueOnce({ code: "FOCUS_NOT_FOUND" });
    await expect(open_floating_focus("a")).rejects.toEqual({
      code: "FOCUS_NOT_FOUND",
    });
    expect(read_floating_focuses()).toEqual([]);
  });

  it("ignores malformed storage", () => {
    storage.set("cardbe.floating-focuses", "{not json");
    expect(read_floating_focuses()).toEqual([]);
    storage.set("cardbe.floating-focuses", JSON.stringify(["a", 1, "a"]));
    expect(read_floating_focuses()).toEqual(["a"]);
  });

  it("restores remembered views and drops deleted ones", async () => {
    storage.set("cardbe.floating-focuses", JSON.stringify(["a", "b", "c"]));
    mocks.invoke.mockImplementation(
      async (_command: string, { id }: { id: string }) => {
        if (id === "b") {
          throw { code: "FOCUS_NOT_FOUND" };
        }
        if (id === "c") {
          throw { code: "INTERNAL_ERROR" };
        }
      },
    );
    await restore_floating_focuses();
    expect(read_floating_focuses()).toEqual(["a", "c"]);
    expect(mocks.warn).toHaveBeenCalledOnce();
  });

  it("decodes the hex view ID from the window URL", () => {
    // "a/ 視" as UTF-8, matching the backend's encoding.
    expect(decode_floating_focus_id("612f20e8a696")).toBe("a/ 視");
    for (const invalid of [null, "", "6", "zz", "ff"]) {
      expect(decode_floating_focus_id(invalid)).toBeNull();
    }
  });
});
