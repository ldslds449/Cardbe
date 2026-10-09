import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";
import type { LinkPreview } from "./link-preview";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const result = (
  url: string,
  status: LinkPreview["status"] = "partial",
): LinkPreview => ({
  url,
  final_url: url,
  domain: "example.com",
  title: "Title",
  description: null,
  image_url: null,
  favicon_url: null,
  status,
  fetched_at: Date.now() / 1000,
});

describe("link preview requests", () => {
  beforeEach(() => {
    vi.resetModules();
    invoke.mockReset();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("fetches only on demand, deduplicates requests and expires unavailable results", async () => {
    const service = await import("./link-preview");
    expect(invoke).not.toHaveBeenCalled();
    const url = "https://example.com/";
    invoke.mockResolvedValue(result(url, "unavailable"));
    const first = service.fetchPreview(url);
    expect(service.fetchPreview(url)).toBe(first);
    await first;
    await service.fetchPreview(url);
    expect(invoke).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(300000);
    expect(service.cachedPreview(url)).toBeUndefined();
    await service.fetchPreview(url);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("keeps out-of-order links separate and retries failed requests", async () => {
    const service = await import("./link-preview");
    let finishA!: (value: LinkPreview) => void;
    invoke.mockImplementationOnce(
      () =>
        new Promise<LinkPreview>((resolve) => {
          finishA = resolve;
        }),
    );
    const a = service.fetchPreview("https://example.com/a");
    invoke.mockResolvedValueOnce(result("https://example.com/b"));
    await service.fetchPreview("https://example.com/b");
    finishA(result("https://example.com/a"));
    await a;
    expect(service.cachedPreview("https://example.com/b")?.url).toBe(
      "https://example.com/b",
    );
    invoke.mockRejectedValueOnce({ code: "INTERNAL_ERROR" });
    await expect(service.fetchPreview("https://example.com/c")).rejects.toEqual(
      { code: "INTERNAL_ERROR" },
    );
    invoke.mockResolvedValueOnce(result("https://example.com/c"));
    await expect(
      service.fetchPreview("https://example.com/c"),
    ).resolves.toHaveProperty("title", "Title");
  });

  it("caches safe image data and retries unavailable images", async () => {
    const service = await import("./link-preview");
    invoke
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce("data:image/png;base64,eA==");
    expect(
      await service.fetchPreviewImage("https://example.com/icon.png"),
    ).toBeNull();
    expect(
      await service.fetchPreviewImage("https://example.com/icon.png"),
    ).toContain("data:image/png");
    await service.fetchPreviewImage("https://example.com/icon.png");
    expect(invoke).toHaveBeenCalledTimes(2);
    invoke.mockRejectedValueOnce({ code: "INTERNAL_ERROR" });
    expect(
      await service.fetchPreviewImage("https://example.com/broken.png"),
    ).toBeNull();
  });

  it("deduplicates image requests and refetches after 24 hours", async () => {
    const service = await import("./link-preview");
    const url = "https://example.com/image.png";
    invoke
      .mockResolvedValueOnce("data:image/png;base64,b2xk")
      .mockResolvedValueOnce("data:image/png;base64,bmV3");
    const first = service.fetchPreviewImage(url);
    expect(service.fetchPreviewImage(url)).toBe(first);
    await first;
    vi.advanceTimersByTime(86400000 - 1);
    expect(service.fetchPreviewImage(url)).toBe(first);
    vi.advanceTimersByTime(1);
    expect(await service.fetchPreviewImage(url)).toBe(
      "data:image/png;base64,bmV3",
    );
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("does not let an expired failed request remove a newer image", async () => {
    const service = await import("./link-preview");
    const url = "https://example.com/image.png";
    let finish!: (value: null) => void;
    invoke.mockImplementationOnce(
      () =>
        new Promise<null>((resolve) => {
          finish = resolve;
        }),
    );
    const expired = service.fetchPreviewImage(url);
    vi.advanceTimersByTime(86400000);
    invoke.mockResolvedValueOnce("data:image/png;base64,bmV3");
    const current = service.fetchPreviewImage(url);
    await current;
    finish(null);
    await expired;
    expect(service.fetchPreviewImage(url)).toBe(current);
    expect(invoke).toHaveBeenCalledTimes(2);
  });
});
