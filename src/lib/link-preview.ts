import { invoke } from "@tauri-apps/api/core";

export interface LinkPreview {
  url: string;
  final_url: string | null;
  domain: string;
  title: string | null;
  description: string | null;
  image_url: string | null;
  favicon_url: string | null;
  status: "success" | "partial" | "unavailable";
  fetched_at: number;
}

const cache = new Map<string, LinkPreview>();
const pending = new Map<string, Promise<LinkPreview>>();
const images = new Map<
  string,
  { request: Promise<string | null>; expiresAt: number }
>();

export function cachedPreview(url: string): LinkPreview | undefined {
  const preview = cache.get(url);
  const ttl = preview?.status === "unavailable" ? 300 : 86400;
  return preview && preview.fetched_at + ttl > Date.now() / 1000
    ? preview
    : undefined;
}

export function fetchPreview(url: string): Promise<LinkPreview> {
  const cached = cachedPreview(url);
  if (cached) {
    return Promise.resolve(cached);
  }
  const existing = pending.get(url);
  if (existing) {
    return existing;
  }
  const request = invoke<LinkPreview>("get_link_preview", { url })
    .then((preview) => {
      if (cache.size >= 100) {
        cache.delete(cache.keys().next().value!);
      }
      cache.set(url, preview);
      return preview;
    })
    .finally(() => pending.delete(url));
  pending.set(url, request);
  return request;
}

export function fetchPreviewImage(url: string): Promise<string | null> {
  const existing = images.get(url);
  if (existing && existing.expiresAt > Date.now()) {
    return existing.request;
  }
  if (images.size >= 32 && !images.has(url)) {
    images.delete(images.keys().next().value!);
  }
  const request = invoke<string | null>("get_link_preview_image", {
    url,
  }).catch(() => null);
  images.set(url, { request, expiresAt: Date.now() + 86400000 });
  // Failed images can be retried on the next hover.
  void request.then((image) => {
    if (!image && images.get(url)?.request === request) {
      images.delete(url);
    }
  });
  return request;
}
