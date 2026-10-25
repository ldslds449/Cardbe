import { beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { resolveImageSource } from "./image-source";
const { isTauri, fetchPreviewImage } = vi.hoisted(() => ({
  isTauri: vi.fn(),
  fetchPreviewImage: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri }));
vi.mock("#lib/link-preview.js", () => ({ fetchPreviewImage }));

describe("editor image loading", () => {
  beforeEach(() => {
    isTauri.mockReset().mockReturnValue(true);
    fetchPreviewImage.mockReset();
  });
  it("uses the existing host image fetcher on desktop", async () => {
    fetchPreviewImage.mockResolvedValue("data:image/png;base64,AAA");
    expect(await resolveImageSource("https://example.com/photo.png")).toBe(
      "data:image/png;base64,AAA",
    );
    expect(fetchPreviewImage).toHaveBeenCalledWith(
      "https://example.com/photo.png",
    );
  });
  it("keeps unavailable images as a safe fallback", async () => {
    fetchPreviewImage.mockResolvedValue(null);
    expect(
      await resolveImageSource("https://example.com/photo.png"),
    ).toBeNull();
  });
  it("allows browser images without desktop IPC", async () => {
    isTauri.mockReturnValue(false);
    expect(await resolveImageSource("https://example.com/photo.png")).toBe(
      "https://example.com/photo.png",
    );
    expect(fetchPreviewImage).not.toHaveBeenCalled();
  });
  it("preserves existing data/asset images and rejects unsafe sources", async () => {
    for (const src of [
      "data:image/png;base64,AAA",
      "asset://localhost/image.png",
    ]) {
      expect(await resolveImageSource(src)).toBe(src);
    }
    for (const src of [
      "javascript:alert(1)",
      "file:///C:/photo.png",
      "data:text/html,x",
      "https://user:secret@example.com/photo.png",
      "",
    ]) {
      expect(await resolveImageSource(src)).toBeNull();
    }
    expect(fetchPreviewImage).not.toHaveBeenCalled();
  });
});
