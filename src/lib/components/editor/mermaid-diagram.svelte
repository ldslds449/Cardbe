<script lang="ts" module>
  // Diagrams by source, so a re-rendered block shows its drawing at once
  // instead of flashing its source and changing height.
  const rendered = new Map<string, { prefix: string; svg: string }>();
  const RENDERED_LIMIT = 200;
  // While the source is being typed, wait for a pause before re-rendering.
  const RENDER_DELAY = 250;
</script>

<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { untrack } from "svelte";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import ZoomableView from "./zoomable-view.svelte";
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
    if (hit) {
      svg = hit;
      failed = false;
      return;
    }
    // Keep the previous drawing until the new one is ready, and while the
    // source is incomplete, so the block does not collapse and shift the
    // text being typed.
    let active = true;
    const timer = setTimeout(
      () =>
        void import("./mermaid")
          .then(({ renderDiagram }) => renderDiagram(text, prefix))
          .then(rendered_svg)
          .catch(() => {
            if (active) {
              failed = true;
            }
          }),
      untrack(() => (svg || failed ? RENDER_DELAY : 0)),
    );
    function rendered_svg(result: { svg: string }) {
      rendered.delete(text);
      rendered.set(text, { prefix, svg: result.svg });
      if (rendered.size > RENDERED_LIMIT) {
        rendered.delete(rendered.keys().next().value!);
      }
      if (active) {
        svg = result.svg;
        failed = false;
      }
    }
    return () => {
      active = false;
      clearTimeout(timer);
    };
  });
</script>

<div
  data-no-drag
  class="mermaid-diagram relative min-w-0 max-w-full"
  contenteditable="false"
  role="figure"
  aria-label={m.editor_mermaid()}
>
  {#if svg}
    <ZoomableView
      label={m.editor_mermaid()}
      class={failed ? "opacity-50" : undefined}
      contentClass="mermaid-theme"
    >
      {@html svg}
    </ZoomableView>
    {#if failed}<p
        class="pointer-events-none absolute bottom-1 left-1 rounded-sm bg-background/90 px-1.5 text-xs text-muted-foreground"
        role="status"
      >
        {m.editor_mermaid_stale()}
      </p>{/if}
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
