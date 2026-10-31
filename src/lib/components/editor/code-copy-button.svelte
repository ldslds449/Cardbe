<script lang="ts">
  import { logger } from "#lib/logger.js";
  import * as m from "#lib/paraglide/messages.js";
  import { toast } from "svelte-sonner";
  import { Button } from "#lib/components/ui/button/index.js";
  import { cn } from "#lib/utils.js";
  import CheckIcon from "@lucide/svelte/icons/check";
  import CopyIcon from "@lucide/svelte/icons/copy";

  // Copies the code of the enclosing `[data-code-block]`, including lines
  // hidden by a collapsed block.
  let { class: className }: { class?: string } = $props();

  let copied = $state(false);
  let reset: ReturnType<typeof setTimeout> | undefined;

  $effect(() => () => clearTimeout(reset));

  async function copy(event: MouseEvent) {
    event.stopPropagation();
    const button = event.currentTarget;
    const code =
      button instanceof HTMLElement
        ? button.closest("[data-code-block]")?.querySelector("pre")?.textContent
        : undefined;
    try {
      await navigator.clipboard.writeText((code ?? "").replace(/\n$/, ""));
      copied = true;
      clearTimeout(reset);
      reset = setTimeout(() => {
        copied = false;
      }, 2000);
    } catch (error) {
      logger.warn("markdown.copy_code.failed", error);
      toast.error(m.markdown_copy_code_error());
    }
  }
</script>

<Button
  type="button"
  data-code-copy
  contenteditable="false"
  variant="ghost"
  size="icon-xs"
  class={cn(
    "bg-background/90 opacity-0 shadow-sm transition-opacity group-hover/code:opacity-100 focus-visible:opacity-100",
    copied && "opacity-100",
    className,
  )}
  aria-label={copied ? m.markdown_code_copied() : m.markdown_copy_code()}
  title={copied ? m.markdown_code_copied() : m.markdown_copy_code()}
  ondblclick={(event) => event.stopPropagation()}
  onclick={copy}
>
  {#if copied}<CheckIcon />{:else}<CopyIcon />{/if}
</Button>
