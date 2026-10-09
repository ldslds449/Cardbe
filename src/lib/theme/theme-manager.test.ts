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
  emit: vi.fn(),
  listen: vi.fn(),
  setMode: vi.fn(),
  setTheme: vi.fn(),
  error: vi.fn(),
  mode: { current: "light" as "light" | "dark" },
}));
vi.mock("$app/environment", () => ({ browser: true }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
  isTauri: () => true,
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: mocks.emit,
  listen: mocks.listen,
}));
vi.mock("mode-watcher", () => ({
  mode: mocks.mode,
  setMode: mocks.setMode,
  setTheme: mocks.setTheme,
}));
vi.mock("svelte-sonner", () => ({ toast: { error: mocks.error } }));
vi.mock("$lib/logger", () => ({ logger: { warn: vi.fn() } }));

describe("theme persistence", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    const storage = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
    });
    vi.stubGlobal("window", {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    mocks.invoke.mockResolvedValue(undefined);
    mocks.emit.mockResolvedValue(undefined);
    mocks.listen.mockResolvedValue(vi.fn());
  });
  afterEach(() => vi.unstubAllGlobals());

  it("applies immediately, saves only confirmed selection, and prevents overlapping writes", async () => {
    const { selectTheme, themeState } = await import("./theme-manager.svelte");
    themeState.ready = true;
    let finish!: () => void;
    mocks.invoke.mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
    );
    const save = selectTheme("morandi-dark-blue");
    expect(themeState.preference).toBe("morandi-dark-blue");
    expect(mocks.setMode).toHaveBeenCalledWith("dark");
    expect(mocks.setTheme).toHaveBeenCalledWith("morandi-dark-blue");
    expect(localStorage.getItem("cardbe-theme")).toBeNull();
    await selectTheme("light");
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    finish();
    await save;
    expect(localStorage.getItem("cardbe-theme")).toBe("morandi-dark-blue");
    expect(mocks.emit).toHaveBeenCalledWith(
      "cardbe:theme-changed",
      "morandi-dark-blue",
    );
  });

  it("rolls back failed persistence without replacing the confirmed cache", async () => {
    localStorage.setItem("cardbe-theme", "morandi-sage");
    const { selectTheme, themeState } = await import("./theme-manager.svelte");
    themeState.ready = true;
    mocks.invoke.mockRejectedValueOnce({ code: "INTERNAL_ERROR" });
    await selectTheme("dark");
    expect(themeState.preference).toBe("morandi-sage");
    expect(mocks.setMode).toHaveBeenLastCalledWith("light");
    expect(mocks.setTheme).toHaveBeenLastCalledWith("morandi-sage");
    expect(localStorage.getItem("cardbe-theme")).toBe("morandi-sage");
    expect(mocks.error).toHaveBeenCalledOnce();
    expect(mocks.emit).not.toHaveBeenCalled();
    expect(themeState.saving).toBe(false);
  });

  it("restores the backend preference and refreshes the startup cache", async () => {
    mocks.invoke.mockResolvedValueOnce({ theme: "morandi-rose" });
    const { initializeTheme, themeState } =
      await import("./theme-manager.svelte");
    const stop = initializeTheme();
    await vi.waitFor(() => expect(themeState.ready).toBe(true));
    expect(themeState.preference).toBe("morandi-rose");
    expect(localStorage.getItem("cardbe-theme")).toBe("morandi-rose");
    stop();
  });

  it("migrates the legacy local appearance without changing other settings", async () => {
    localStorage.setItem("mode-watcher-mode", "dark");
    mocks.invoke.mockResolvedValueOnce({ language: "zh-TW" });
    const { initializeTheme, themeState } =
      await import("./theme-manager.svelte");
    const stop = initializeTheme();
    await vi.waitFor(() => expect(themeState.ready).toBe(true));
    expect(themeState.preference).toBe("dark");
    expect(mocks.invoke).toHaveBeenCalledWith("set_theme", { theme: "dark" });
    stop();
  });

  it("does not overwrite a selection with a stale settings read", async () => {
    mocks.invoke.mockResolvedValueOnce({ theme: "light" });
    const { initializeTheme, selectTheme, themeState } =
      await import("./theme-manager.svelte");
    const stop = initializeTheme();
    await vi.waitFor(() => expect(themeState.ready).toBe(true));
    let finish!: (settings: { theme: string }) => void;
    mocks.invoke.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const refresh = vi
      .mocked(window.addEventListener)
      .mock.calls.find(
        (call) => call[0] === "focus",
      )![1] as () => Promise<void>;
    const reading = refresh();
    await selectTheme("morandi-blue");
    finish({ theme: "light" });
    await reading;
    expect(themeState.preference).toBe("morandi-blue");
    stop();
  });

  it("updates other windows and cleans up listeners", async () => {
    const unlisten = vi.fn();
    mocks.listen.mockResolvedValueOnce(unlisten);
    mocks.invoke.mockResolvedValueOnce({ theme: "light" });
    const { initializeTheme, themeState } =
      await import("./theme-manager.svelte");
    const stop = initializeTheme();
    await vi.waitFor(() => expect(themeState.ready).toBe(true));
    const receive = mocks.listen.mock.calls[0][1] as (event: {
      payload: unknown;
    }) => void;
    mocks.invoke.mockResolvedValueOnce({ theme: "morandi-dark-mauve" });
    receive({ payload: "light" });
    await vi.waitFor(() =>
      expect(themeState.preference).toBe("morandi-dark-mauve"),
    );
    expect(localStorage.getItem("cardbe-theme")).toBe("morandi-dark-mauve");
    receive({ payload: "invalid" });
    expect(themeState.preference).toBe("morandi-dark-mauve");
    stop();
    expect(unlisten).toHaveBeenCalledOnce();
    expect(window.removeEventListener).toHaveBeenCalledWith(
      "focus",
      expect.any(Function),
    );
  });
});
