<script lang="ts">
  import "katex/dist/katex.min.css";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { renderMath } from "./math";
  let { source, display = false }: { source: string; display?: boolean } =
    $props();
  const html = $derived(renderMath(source, display));
</script>

{#snippet formula()}
  {#if html}
    {@html html}
  {:else}
    <span class="whitespace-pre-wrap font-mono">{source}</span>
  {/if}
{/snippet}

{#if display}
  <ScrollArea
    orientation="horizontal"
    class="math-scroll min-w-0 max-w-full overflow-hidden"
  >
    <div class="w-max min-w-full">{@render formula()}</div>
  </ScrollArea>
{:else}
  <span class="math-inline">{@render formula()}</span>
{/if}
