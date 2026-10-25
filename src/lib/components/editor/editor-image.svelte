<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { resolveImageSource } from "./image-source";
  import type { HTMLImgAttributes } from "svelte/elements";
  let {
    src,
    alt,
    title,
    width,
    height,
    class: className,
  }: {
    src?: string | null;
    alt?: string | null;
    title?: string | null;
    width?: number | string | null;
    height?: number | string | null;
    class?: HTMLImgAttributes["class"];
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

{#if image}
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
