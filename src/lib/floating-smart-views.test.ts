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
vi.mock("$lib/logger", () => ({ logger: { warn: mocks.warn } }));

import {
  decode_floating_smart_view_id,
  forget_floating_smart_view,
  open_floating_smart_view,
  read_floating_smart_views,
  restore_floating_smart_views,
} from "./floating-smart-views";

describe("floating smart view windows", () => {
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

  it("remembers each opened view once and forgets closed ones", async () => {
    await open_floating_smart_view("a");
    await open_floating_smart_view("a");
    await open_floating_smart_view("b");
    expect(mocks.invoke).toHaveBeenCalledWith("open_floating_smart_view", {
      id: "a",
    });
    expect(read_floating_smart_views()).toEqual(["a", "b"]);
    forget_floating_smart_view("a");
    expect(read_floating_smart_views()).toEqual(["b"]);
  });

  it("does not remember a window that failed to open", async () => {
    mocks.invoke.mockRejectedValueOnce({ code: "SMART_VIEW_NOT_FOUND" });
    await expect(open_floating_smart_view("a")).rejects.toEqual({
      code: "SMART_VIEW_NOT_FOUND",
    });
    expect(read_floating_smart_views()).toEqual([]);
  });

  it("ignores malformed storage", () => {
    storage.set("cardbe.floating-smart-views", "{not json");
    expect(read_floating_smart_views()).toEqual([]);
    storage.set("cardbe.floating-smart-views", JSON.stringify(["a", 1, "a"]));
    expect(read_floating_smart_views()).toEqual(["a"]);
  });

  it("restores remembered views and drops deleted ones", async () => {
    storage.set("cardbe.floating-smart-views", JSON.stringify(["a", "b", "c"]));
    mocks.invoke.mockImplementation(
      async (_command: string, { id }: { id: string }) => {
        if (id === "b") {
          throw { code: "SMART_VIEW_NOT_FOUND" };
        }
        if (id === "c") {
          throw { code: "INTERNAL_ERROR" };
        }
      },
    );
    await restore_floating_smart_views();
    expect(read_floating_smart_views()).toEqual(["a", "c"]);
    expect(mocks.warn).toHaveBeenCalledOnce();
  });

  it("decodes the hex view ID from the window URL", () => {
    // "a/ 視" as UTF-8, matching the backend's encoding.
    expect(decode_floating_smart_view_id("612f20e8a696")).toBe("a/ 視");
    for (const invalid of [null, "", "6", "zz", "ff"]) {
      expect(decode_floating_smart_view_id(invalid)).toBeNull();
    }
  });
});
