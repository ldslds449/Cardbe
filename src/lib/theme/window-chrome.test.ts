import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  setTheme: vi.fn(),
  isTauri: vi.fn(() => true),
  frames: [] as FrameRequestCallback[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
  isTauri: mocks.isTauri,
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTheme: mocks.setTheme }),
}));
vi.mock("$lib/logger", () => ({ logger: { warn: vi.fn() } }));

describe("window chrome", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    mocks.frames.length = 0;
    mocks.isTauri.mockReturnValue(true);
    mocks.invoke.mockResolvedValue(undefined);
    mocks.setTheme.mockResolvedValue(undefined);
    const tokens: Record<string, string> = {
      "--background": "#102030",
      "--foreground": "#f0e0d0",
    };
    const pixels: Record<string, number[]> = {
      "#102030": [16, 32, 48, 255],
      "#f0e0d0": [240, 224, 208, 255],
    };
    const context = {
      fillStyle: "",
      clearRect: vi.fn(),
      fillRect: vi.fn(),
      getImageData: () => ({ data: pixels[context.fillStyle] }),
    };
    vi.stubGlobal("document", {
      documentElement: {},
      createElement: () => ({ getContext: () => context }),
    });
    vi.stubGlobal("getComputedStyle", () => ({
      getPropertyValue: (token: string) => ` ${tokens[token]}`,
    }));
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) =>
      mocks.frames.push(callback),
    );
  });
  afterEach(() => vi.unstubAllGlobals());

  it("follows fixed theme appearance and keeps System on the OS theme", async () => {
    const { syncWindowChrome } = await import("./window-chrome");
    syncWindowChrome("morandi-dark-blue");
    syncWindowChrome("morandi-sage");
    syncWindowChrome("system");
    expect(mocks.setTheme.mock.calls).toEqual([["dark"], ["light"], [null]]);
  });

  it("sends the latest theme tokens as caption colors", async () => {
    const { syncWindowChrome } = await import("./window-chrome");
    syncWindowChrome("light");
    syncWindowChrome("dark");
    mocks.frames.forEach((frame) => frame(0));
    expect(mocks.invoke).toHaveBeenCalledOnce();
    expect(mocks.invoke).toHaveBeenCalledWith("set_window_caption_color", {
      caption: [16, 32, 48],
      text: [240, 224, 208],
    });
  });

  it("does nothing outside the desktop app", async () => {
    mocks.isTauri.mockReturnValue(false);
    const { syncWindowChrome } = await import("./window-chrome");
    syncWindowChrome("dark");
    expect(mocks.setTheme).not.toHaveBeenCalled();
    expect(mocks.frames).toHaveLength(0);
  });
});
