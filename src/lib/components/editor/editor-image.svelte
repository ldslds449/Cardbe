<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { resolveImageSource } from "./image-source";
  import ZoomableView from "./zoomable-view.svelte";
  import type { HTMLImgAttributes } from "svelte/elements";
  let {
    src,
    alt,
    title,
    width,
    height,
    class: className,
    zoomable = false,
  }: {
    src?: string | null;
    alt?: string | null;
    title?: string | null;
    width?: number | string | null;
    height?: number | string | null;
    class?: HTMLImgAttributes["class"];
    zoomable?: boolean;
  } = $props();
  let image = $state<string | null>(null);
  let loading = $state(false);

  $effect(() => {
    const source = src;
    let active = true;
    image = null;
    loading = !!source;
    if (source) {
      void resolveImageSource(source).then((value) => {
        if (active) {
          image = value;
          loading = false;
        }
      });
    }
    return () => {
      active = false;
    };
  });
</script>

{#snippet picture(source: string)}
  <img
    src={source}
    alt={alt ?? ""}
    title={title ?? undefined}
    width={width ?? undefined}
    height={height ?? undefined}
    class={className}
    referrerpolicy="no-referrer"
    onerror={() => {
      image = null;
    }}
  />
{/snippet}

{#if image && zoomable}
  <ZoomableView label={alt || m.editor_image()} inline>
    {@render picture(image)}
  </ZoomableView>
{:else if image}
  <img
    src={image}
    alt={alt ?? ""}
    title={title ?? undefined}
    width={width ?? undefined}
    height={height ?? undefined}
    class={className}
    referrerpolicy="no-referrer"
    onerror={() => {
      image = null;
    }}
  />
{:else}
  <span
    class="inline-block rounded-md border px-2 py-1 text-sm text-muted-foreground"
    role="img"
    aria-label={loading
      ? m.editor_image_loading()
      : m.editor_image_unavailable()}
    >{alt ||
      (loading ? m.editor_image_loading() : m.editor_image_unavailable())}</span
  >
{/if}
