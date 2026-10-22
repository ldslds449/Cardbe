<script lang="ts" module>
  import type { Component } from "svelte";

  // Reuse the loaded renderer so later mounts render without the plain-text
  // fallback, which has a different height and would shift restored scroll.
  let loadedRenderer: Component<{ md: string; compact?: boolean }> | null =
    null;
</script>

<script lang="ts">
  import { logger } from "$lib/logger";
  import { onMount } from "svelte";
  import { render_card_references } from "../utils/card-reference";

  let {
    md,
    compact = false,
  }: {
    md: string;
    compact?: boolean;
  } = $props();

  let Renderer = $state(loadedRenderer);
  const rendered_md = $derived(render_card_references(md));

  onMount(() => {
    if (Renderer) {
      return;
    }
    let active = true;
    void import("./markdown_renderer.svelte")
      .then((module) => {
        loadedRenderer = module.default;
        if (active) {
          Renderer = module.default;
        }
      })
      .catch((error) => {
        logger.warn("markdown.renderer_load.failed", error);
      });
    return () => {
      active = false;
    };
  });
</script>

{#if Renderer}
  <Renderer md={rendered_md} {compact} />
{:else}
  <div
    class="whitespace-pre-wrap {compact ? 'text-sm leading-5' : 'leading-6'}"
  >
    {md}
  </div>
{/if}
