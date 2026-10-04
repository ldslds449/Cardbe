<script lang="ts">
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  let {
    contentDOM = $bindable<HTMLElement | null>(null),
  }: { contentDOM?: HTMLElement | null } = $props();
</script>

<ScrollArea
  type="auto"
  orientation="horizontal"
  class="code-scroll min-w-0 w-full max-w-full overflow-hidden rounded-md border"
>
  <pre><code bind:this={contentDOM}></code></pre>
</ScrollArea>
