<script lang="ts">
  import { onMount } from "svelte";
  import type { Component } from "svelte";
  import { render_card_references } from "../utils/card-reference";

  let {
    md,
    compact = false,
  }: {
    md: string;
    compact?: boolean;
  } = $props();

  let Renderer = $state<Component<{ md: string; compact?: boolean }> | null>(null);
  const rendered_md = $derived(render_card_references(md));

  onMount(() => {
    let active = true;
    void import("./markdown_renderer.svelte")
      .then((module) => {
        if (active) Renderer = module.default;
      })
      .catch((error) => {
        console.error("Couldn't load the Markdown renderer", error);
      });
    return () => {
      active = false;
    };
  });
</script>

{#if Renderer}
  <Renderer md={rendered_md} {compact} />
{:else}
  <div class="whitespace-pre-wrap {compact ? 'text-sm leading-5' : 'leading-6'}">{md}</div>
{/if}
