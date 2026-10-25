import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";
import { applyLanguagePreference } from "#lib/i18n/index.js";
import {
  format_cpu,
  format_memory,
  HISTORY_LENGTH,
  ResourceMonitor,
  type ResourceUsage,
} from "./resource-usage.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function sample(cpu: number | null, memory_mib: number): ResourceUsage {
  const memory_bytes = memory_mib * 1024 * 1024;
  return {
    total: { cpu_percent: cpu, memory_bytes, process_count: 5 },
    main: { cpu_percent: cpu, memory_bytes, process_count: 1 },
    children: { cpu_percent: 0, memory_bytes: 0, process_count: 4 },
    logical_cpus: 8,
    memory_metric: "private_working_set",
  };
}

/** Lets awaited fetches settle without advancing timers. */
const settle = () => vi.advanceTimersByTimeAsync(0);

describe("ResourceMonitor", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("shares one polling loop and stops it with the last subscriber", async () => {
    const fetch = vi.fn(async () => sample(2.4, 128));
    const monitor = new ResourceMonitor({ fetch });
    const leave_first = monitor.subscribe();
    const leave_second = monitor.subscribe();
    await settle();
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(monitor.status).toBe("ready");
    expect(monitor.usage?.total.cpu_percent).toBe(2.4);

    await vi.advanceTimersByTimeAsync(3000);
    expect(fetch).toHaveBeenCalledTimes(4);
    expect(vi.getTimerCount()).toBe(1);

    leave_first();
    leave_first();
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetch).toHaveBeenCalledTimes(5);

    leave_second();
    expect(vi.getTimerCount()).toBe(0);
    expect(monitor.polling).toBe(false);
    expect(monitor.usage).toBeNull();
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetch).toHaveBeenCalledTimes(5);
  });

  it("remounting resumes a single loop", async () => {
    const fetch = vi.fn(async () => sample(1, 64));
    const monitor = new ResourceMonitor({ fetch });
    for (let mount = 0; mount < 5; mount++) {
      monitor.subscribe()();
    }
    const leave = monitor.subscribe();
    await settle();
    await vi.advanceTimersByTimeAsync(2000);
    // One poll per remount that found no loop running, then one per second.
    expect(vi.getTimerCount()).toBe(1);
    const calls = fetch.mock.calls.length;
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetch).toHaveBeenCalledTimes(calls + 1);
    leave();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("never overlaps requests", async () => {
    let resolve: (usage: ResourceUsage) => void = () => {};
    const fetch = vi.fn(
      () => new Promise<ResourceUsage>((done) => (resolve = done)),
    );
    const monitor = new ResourceMonitor({ fetch });
    const leave = monitor.subscribe();
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(monitor.status).toBe("loading");

    // Leaving and returning while a request runs reuses it.
    leave();
    const leave_again = monitor.subscribe();
    expect(fetch).toHaveBeenCalledTimes(1);
    resolve(sample(3, 100));
    await settle();
    expect(monitor.status).toBe("ready");
    await vi.advanceTimersByTimeAsync(1000);
    expect(fetch).toHaveBeenCalledTimes(2);
    leave_again();
  });

  it("shows failures as unavailable without console errors and keeps polling", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const fetch = vi
      .fn<() => Promise<ResourceUsage>>()
      .mockRejectedValueOnce({ code: "RESOURCE_USAGE_UNAVAILABLE" })
      .mockRejectedValueOnce(new Error("boom"))
      .mockResolvedValue(sample(5, 200));
    const monitor = new ResourceMonitor({ fetch });
    const leave = monitor.subscribe();
    await settle();
    expect(monitor.status).toBe("unavailable");
    expect(monitor.usage).toBeNull();
    await vi.advanceTimersByTimeAsync(1000);
    expect(monitor.status).toBe("unavailable");
    await vi.advanceTimersByTimeAsync(1000);
    expect(monitor.status).toBe("ready");
    expect(error).not.toHaveBeenCalled();
    leave();
    error.mockRestore();
  });

  it("keeps a bounded history only while it is tracked", async () => {
    let cpu = 0;
    const fetch = vi.fn(async () => sample(cpu++, 100));
    const monitor = new ResourceMonitor({ fetch });
    const leave = monitor.subscribe();
    await vi.advanceTimersByTimeAsync(3000);
    expect(monitor.history).toEqual([]);

    const stop = monitor.track_history();
    await vi.advanceTimersByTimeAsync((HISTORY_LENGTH + 10) * 1000);
    expect(monitor.history).toHaveLength(HISTORY_LENGTH);
    expect(monitor.history.at(-1)?.cpu_percent).toBe(cpu - 1);
    expect(monitor.history.at(-1)?.memory_bytes).toBe(100 * 1024 * 1024);

    stop();
    expect(monitor.history).toEqual([]);
    await vi.advanceTimersByTimeAsync(2000);
    expect(monitor.history).toEqual([]);
    leave();
  });

  it("pauses while the page is hidden", async () => {
    const listeners = new Set<() => void>();
    const document = {
      visibilityState: "visible",
      addEventListener: (_: string, listener: () => void) =>
        listeners.add(listener),
      removeEventListener: (_: string, listener: () => void) =>
        listeners.delete(listener),
    };
    vi.stubGlobal("document", document);
    const fetch = vi.fn(async () => sample(1, 50));
    const monitor = new ResourceMonitor({ fetch });
    const leave = monitor.subscribe();
    await settle();
    expect(fetch).toHaveBeenCalledTimes(1);

    document.visibilityState = "hidden";
    listeners.forEach((listener) => listener());
    await vi.advanceTimersByTimeAsync(5000);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);

    document.visibilityState = "visible";
    listeners.forEach((listener) => listener());
    await settle();
    expect(fetch).toHaveBeenCalledTimes(2);

    leave();
    expect(listeners.size).toBe(0);
  });
});

describe("resource usage formatting", () => {
  beforeEach(() => applyLanguagePreference("en"));
  afterEach(() => applyLanguagePreference("system"));

  it("formats CPU with one decimal and unavailable values", () => {
    expect(format_cpu(2.4)).toBe("2.4%");
    expect(format_cpu(0)).toBe("0.0%");
    expect(format_cpu(100)).toBe("100.0%");
    expect(format_cpu(null)).toBe("--");
    expect(format_cpu(undefined)).toBe("--");
    expect(format_cpu(Number.NaN)).toBe("--");
  });

  it("switches memory from MB to GB", () => {
    const mib = 1024 * 1024;
    expect(format_memory(128 * mib)).toBe("128 MB");
    expect(format_memory(1023.4 * mib)).toBe("1,023 MB");
    expect(format_memory(1280 * mib)).toBe("1.25 GB");
    expect(format_memory(null)).toBe("--");
  });

  it("follows the active locale", () => {
    applyLanguagePreference("zh-TW");
    expect(format_cpu(12.5)).toBe("12.5%");
    expect(format_memory(2048 * 1024 * 1024)).toMatch(/^2\.00\s?GB$/);
  });
});
