import { isTauri } from "@tauri-apps/api/core";
import { fetchPreviewImage } from "#lib/link-preview.js";

export async function resolveImageSource(
  source: string,
): Promise<string | null> {
  try {
    const url = new URL(source);
    if (
      ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password
    ) {
      // shortcut: reuse the preview fetcher's 512 KiB raster limit; add a content-image fetcher for larger images.
      return isTauri() ? fetchPreviewImage(url.href) : url.href;
    }
    return url.protocol === "asset:" ||
      (url.protocol === "data:" && /^data:image\//i.test(source))
      ? source
      : null;
  } catch {
    return null;
  }
}
