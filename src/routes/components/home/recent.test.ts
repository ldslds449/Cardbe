import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";
import { by_recent, mark_recent, read_recent } from "./recent";

describe("recently viewed order", () => {
  let storage: Map<string, string>;

  beforeEach(() => {
    storage = new Map();
    vi.stubGlobal("window", {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
      },
    });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("lists recent items first and keeps the rest in their order", () => {
    vi.useFakeTimers();
    vi.setSystemTime(100);
    mark_recent("board", 3);
    vi.setSystemTime(200);
    mark_recent("board", 1);
    mark_recent("focus", "a");

    const recent = read_recent();
    expect(recent.board).toEqual({ 1: 200, 3: 100 });
    expect(recent.note).toEqual({});
    expect(by_recent([4, 3, 2, 1], (id) => recent.board[id])).toEqual([
      1, 3, 4, 2,
    ]);
  });

  it("falls back to the default order when storage is unusable", () => {
    storage.set("cardbe.recent", "{broken");
    expect(read_recent()).toEqual({ focus: {}, board: {}, note: {} });
    storage.set("cardbe.recent", JSON.stringify({ board: { 1: "soon" } }));
    expect(read_recent().board).toEqual({});
    vi.stubGlobal("window", {
      localStorage: {
        getItem: () => {
          throw new Error("blocked");
        },
        setItem: () => {
          throw new Error("blocked");
        },
      },
    });
    expect(() => mark_recent("note", 1)).not.toThrow();
  });
});
