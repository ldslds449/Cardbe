<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  let { source, showSource = true }: { source: string; showSource?: boolean } =
    $props();
  const id = $props.id();
  let svg = $state("");
  let failed = $state(false);

  $effect(() => {
    const text = source;
    let active = true;
    svg = "";
    failed = false;
    void import("./mermaid")
      .then(({ renderDiagram }) =>
        renderDiagram(text, `diagram-${id.replace(/[^a-zA-Z0-9_-]/g, "")}`),
      )
      .then((result) => {
        if (active) {
          svg = result.svg;
        }
      })
      .catch(() => {
        if (active) {
          failed = true;
        }
      });
    return () => {
      active = false;
    };
  });
</script>

<div
  data-no-drag
  class="mermaid-diagram min-w-0 max-w-full"
  contenteditable="false"
  role="figure"
  aria-label={m.editor_mermaid()}
>
  {#if svg}
    <ScrollArea
      orientation="horizontal"
      class="min-w-0 max-w-full overflow-hidden"
    >
      {@html svg}
    </ScrollArea>
  {:else}
    {#if failed}<p class="text-sm text-muted-foreground" role="status">
        {m.editor_mermaid_error()}
      </p>{/if}
    {#if showSource}<ScrollArea
        orientation="horizontal"
        class="code-scroll min-w-0 max-w-full overflow-hidden rounded-md border"
      >
        <pre><code>{source}</code></pre>
      </ScrollArea>{/if}
  {/if}
</div>
