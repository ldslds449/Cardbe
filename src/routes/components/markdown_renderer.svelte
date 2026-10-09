<script lang="ts">
  import { logger } from "$lib/logger";
  import "$lib/components/editor/markdown-content.css";
  import * as m from "$lib/paraglide/messages.js";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { toast } from "svelte-sonner";

  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import Link2Icon from "@lucide/svelte/icons/link-2";
  import LinkPreview from "$lib/components/link-preview.svelte";

  import Markdown from "svelte-exmarkdown";
  import type { HastNode, Plugin } from "svelte-exmarkdown";
  import { gfmPlugin } from "svelte-exmarkdown/gfm";

  // code highlight
  import rehypeShikiFromHighlighter from "@shikijs/rehype/core";

  import { getHighlighter, loadHighlightLanguage } from "./highlighter.svelte";
  import { task_id_from_card_reference } from "../utils/card-reference";

  let {
    md,
    compact = false,
  }: {
    md: string;
    compact?: boolean;
  } = $props();

  const sharedHighlighter = getHighlighter();
  let highlight_revision = $state(0);

  // Exmarkdown parses synchronously. Load grammars in the background, then
  // rebuild the plugin so the current Markdown is highlighted on completion.
  function load_code_languages(): (tree: HastNode) => void {
    return (tree) => {
      const pending = new Set<Promise<void>>();
      const visit = (node: HastNode) => {
        if (node.type === "element" && node.tagName === "code") {
          const classes = node.properties?.className;
          if (Array.isArray(classes)) {
            for (const name of classes) {
              if (typeof name !== "string" || !name.startsWith("language-")) {
                continue;
              }
              const load = loadHighlightLanguage(
                name.slice("language-".length),
              );
              if (load) {
                pending.add(load);
              }
            }
          }
        }
        node.children?.forEach(visit);
      };
      visit(tree);
      if (pending.size) {
        void Promise.allSettled(pending).then((results) => {
          if (results.some((result) => result.status === "fulfilled")) {
            highlight_revision += 1;
          }
          for (const result of results) {
            if (result.status === "rejected") {
              console.error("Couldn't load code highlighting", result.reason);
              logger.warn("markdown.highlight_load.failed", result.reason);
            }
          }
        });
      }
    };
  }

  const languageLoaderPlugin: Plugin = { rehypePlugin: load_code_languages };

  // Task descriptions are also rendered by the unauthenticated LAN viewer.
  // Keep Markdown as data: raw HTML is not part of the supported format and
  // must not acquire rendering semantics through a dependency update.
  function strip_raw_html(): (tree: HastNode) => void {
    return (tree) => {
      const visit = (node: HastNode) => {
        if (!node.children) {
          return;
        }
        node.children = node.children.filter((child) => child.type !== "raw");
        node.children.forEach(visit);
      };
      visit(tree);
    };
  }

  function safe_external_href(href: unknown): string | undefined {
    if (typeof href !== "string") {
      return undefined;
    }
    if (task_id_from_card_reference(href)) {
      return href;
    }

    try {
      const url = new URL(href);
      return url.protocol === "https:" || url.protocol === "http:"
        ? url.href
        : undefined;
    } catch {
      return undefined;
    }
  }

  function show_card_reference_preview(event: Event, task_id: string) {
    const target = event.currentTarget;
    if (!(target instanceof HTMLElement)) {
      return;
    }
    const rect = target.getBoundingClientRect();
    window.dispatchEvent(
      new CustomEvent("cardbe:show-card-preview", {
        detail: {
          taskId: task_id,
          left: rect.left,
          top: rect.top,
          bottom: rect.bottom,
        },
      }),
    );
  }

  function hide_card_reference_preview() {
    window.dispatchEvent(new CustomEvent("cardbe:hide-card-preview"));
  }

  function prevent_task_drag(node: HTMLElement) {
    const stop_pointerdown = (event: PointerEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest('[data-slot="scroll-area-scrollbar"]')
      ) {
        return;
      }
      event.stopPropagation();
    };
    node.addEventListener("pointerdown", stop_pointerdown);

    return {
      destroy() {
        node.removeEventListener("pointerdown", stop_pointerdown);
      },
    };
  }

  async function open_external_link(event: MouseEvent, href: unknown) {
    event.stopPropagation();
    const safe_href = safe_external_href(href);
    if (!safe_href) {
      event.preventDefault();
      return;
    }

    const task_id = task_id_from_card_reference(safe_href);
    if (task_id) {
      event.preventDefault();
      window.dispatchEvent(
        new CustomEvent("cardbe:open-card", { detail: { taskId: task_id } }),
      );
      return;
    }

    // In a regular browser, keep the anchor's native target="_blank" behavior.
    // The Tauri app uses the system opener through a validated backend command.
    if (!isTauri()) {
      return;
    }

    event.preventDefault();
    try {
      await invoke("open_external_url", { url: safe_href });
    } catch (error) {
      logger.warn("markdown.open_link.failed", error);
      console.error("Couldn't open external link", error);
      toast.error(m.link_open_error());
    }
  }

  const shikiPlugin = $derived.by(() => {
    void highlight_revision;

    return {
      rehypePlugin: [
        rehypeShikiFromHighlighter,
        sharedHighlighter, // The same instance is reused here
        {
          theme: "cardbe",
          fallbackLanguage: "text",
        },
      ],
    } satisfies Plugin;
  });

  const safeMarkdownPlugin: Plugin = {
    rehypePlugin: strip_raw_html,
  };
</script>

<div
  class="markdown-content min-w-0 max-w-full [&_table]:table-fixed"
  class:markdown-content-compact={compact}
>
  <Markdown
    {md}
    plugins={[
      gfmPlugin(),
      safeMarkdownPlugin,
      languageLoaderPlugin,
      shikiPlugin,
    ]}
  >
    {#snippet pre(props)}
      {@const { children, class: className, ...rest } = props}
      <div use:prevent_task_drag>
        <ScrollArea
          type="auto"
          orientation="horizontal"
          class="code-scroll min-w-0 w-full max-w-full overflow-hidden rounded-md border"
        >
          <pre class={className ?? ""} {...rest}>{@render children?.()}</pre>
        </ScrollArea>
      </div>
    {/snippet}
    {#snippet table(props)}
      {@const { children, style, class: className, ...rest } = props}
      <table class="{className} w-full border-collapse text-sm" {...rest}>
        {@render children?.()}
      </table>
    {/snippet}
    {#snippet tr(props)}
      {@const { children, style, class: className, ...rest } = props}
      <tr class="{className} m-0 border-t p-0 even:bg-muted/40" {...rest}>
        {@render children?.()}
      </tr>
    {/snippet}
    {#snippet th(props)}
      {@const { children, style, class: className, ...rest } = props}
      <th
        class="{className} border bg-muted/60 px-3 py-2 text-left font-semibold [&[align=center]]:text-center [&[align=right]]:text-right"
        {...rest}
      >
        {@render children?.()}
      </th>
    {/snippet}
    {#snippet td(props)}
      {@const { children, style, class: className, ...rest } = props}
      <td
        class="{className} border px-3 py-2 text-left [&[align=center]]:text-center [&[align=right]]:text-right"
        {...rest}
      >
        {@render children?.()}
      </td>
    {/snippet}
    {#snippet small(props)}
      {@const { children, style, class: className, ...rest } = props}
      <small class="{className} text-sm font-medium leading-none" {...rest}>
        {@render children?.()}
      </small>
    {/snippet}
    {#snippet a(props)}
      {@const { children, style, class: className, href, ...rest } = props}
      {@const task_id = task_id_from_card_reference(href)}
      {@const safe_href = safe_external_href(href)}
      {#if task_id}
        <a
          href={safe_href}
          class="{className} inline-flex max-w-full items-baseline gap-1 align-baseline whitespace-normal font-medium text-primary underline decoration-primary/70 decoration-1 underline-offset-4 transition-colors hover:text-primary/80 hover:decoration-2"
          onmouseenter={(event) => show_card_reference_preview(event, task_id)}
          onmouseleave={hide_card_reference_preview}
          onfocus={(event) => show_card_reference_preview(event, task_id)}
          onblur={hide_card_reference_preview}
          onpointerdown={(event) => event.stopPropagation()}
          onclick={(event) => void open_external_link(event, href)}
        >
          <Link2Icon class="size-[1em] shrink-0 self-center" />
          <span class="min-w-0">{@render children?.()}</span>
        </a>
      {:else}
        <LinkPreview
          {...rest}
          href={safe_href}
          class="{className} font-medium text-primary underline underline-offset-4 hover:opacity-80"
          target="_blank"
          rel="noreferrer noopener"
          onpointerdown={(event) => event.stopPropagation()}
          onclick={(event) => void open_external_link(event, href)}
        >
          {@render children?.()}
        </LinkPreview>
      {/if}
    {/snippet}
    {#snippet input(props)}
      {@const { children, style, type, checked, disabled, ...rest } = props}
      {#if type === "checkbox"}
        <Checkbox
          class="align-middle inline-block align-baseline"
          checked={checked ?? undefined}
          disabled={disabled ?? undefined}
          {style}
        />
      {:else}
        <input {type} {...rest} />
      {/if}
    {/snippet}
  </Markdown>
</div>
