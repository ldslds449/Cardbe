<script lang="ts" module>
  // Diagrams by source, so a re-rendered block shows its drawing at once
  // instead of flashing its source and changing height.
  const rendered = new Map<string, { prefix: string; svg: string }>();
  const RENDERED_LIMIT = 200;
</script>

<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { untrack } from "svelte";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  let { source, showSource = true }: { source: string; showSource?: boolean } =
    $props();
  const id = $props.id();
  const prefix = `diagram-${id.replace(/[^a-zA-Z0-9_-]/g, "")}`;

  // Element IDs inside the SVG stay unique to this instance.
  function cached(text: string): string {
    const hit = rendered.get(text);
    return hit ? hit.svg.replaceAll(hit.prefix, prefix) : "";
  }

  let svg = $state(untrack(() => cached(source)));
  let failed = $state(false);

  $effect(() => {
    const text = source;
    const hit = cached(text);
    failed = false;
    if (hit) {
      svg = hit;
      return;
    }
    // Keep the previous drawing until the new one is ready.
    let active = true;
    void import("./mermaid")
      .then(({ renderDiagram }) => renderDiagram(text, prefix))
      .then((result) => {
        rendered.delete(text);
        rendered.set(text, { prefix, svg: result.svg });
        if (rendered.size > RENDERED_LIMIT) {
          rendered.delete(rendered.keys().next().value!);
        }
        if (active) {
          svg = result.svg;
        }
      })
      .catch(() => {
        if (active) {
          svg = "";
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
