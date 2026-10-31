<script lang="ts">
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import type { Readable } from "svelte/store";
  import CodeCopyButton from "./code-copy-button.svelte";
  import MermaidDiagram from "./mermaid-diagram.svelte";
  let {
    contentDOM = $bindable<HTMLElement | null>(null),
    content,
  }: {
    contentDOM?: HTMLElement | null;
    content: Readable<{ source: string; language: string }>;
  } = $props();
</script>

{#if $content.language === "mermaid"}
  <MermaidDiagram source={$content.source} showSource={false} />
{/if}
<div data-code-block class="code-frame group/code">
  <ScrollArea
    type="auto"
    orientation="horizontal"
    class="code-scroll min-w-0 w-full max-w-full overflow-hidden rounded-md border"
  >
    <pre><code bind:this={contentDOM}></code></pre>
  </ScrollArea>
  <CodeCopyButton class="absolute top-1.5 right-1.5 z-10" />
</div>
