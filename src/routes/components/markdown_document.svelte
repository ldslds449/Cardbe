<script lang="ts">
  import { cn } from "#lib/utils.js";
  import { m } from "#lib/paraglide/messages.js";
  import { tick } from "svelte";
  import { Button } from "#lib/components/ui/button/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import * as Collapsible from "#lib/components/ui/collapsible/index.js";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";

  import Markdown from "./markdown.svelte";
  import MarkdownOutlineMenu from "./markdown_outline_menu.svelte";
  import {
    locate_rendered_text,
    markdown_outline,
    split_markdown_sections,
    type MarkdownHeading,
    type TextTarget,
  } from "../utils/markdown-outline";

  // Long Markdown with an outline, collapsible top-level sections, and
  // double-click to edit at the clicked block. With `fill`, the document
  // fills its parent and scrolls below a fixed header; otherwise the header
  // sticks to the top of the surrounding scroll area.
  let {
    md,
    label,
    onedit,
    fill = false,
  }: {
    md: string;
    label: string;
    onedit?: (target: TextTarget) => void;
    fill?: boolean;
  } = $props();

  const HEADING_SELECTOR = "h1, h2, h3, h4, h5, h6";
  const BLOCK_SELECTOR = `p, li, td, th, pre, blockquote, ${HEADING_SELECTOR}`;

  let container = $state<HTMLDivElement | null>(null);
  const outline = $derived(markdown_outline(md));
  const sections = $derived(split_markdown_sections(md, outline));
  let collapsed = $derived(sections.map(() => false));

  function section_index(heading: MarkdownHeading): number {
    return sections.findIndex(
      (section) =>
        heading.start >= section.start && heading.start < section.end,
    );
  }

  function set_collapsed(index: number, value: boolean) {
    collapsed = collapsed.map((current, i) => (i === index ? value : current));
  }

  async function go_to_heading(heading: MarkdownHeading) {
    const index = section_index(heading);
    if (index === -1) {
      return;
    }
    set_collapsed(index, false);
    await tick();
    const section = container?.querySelector(`[data-section="${index}"]`);
    const position = outline.filter(
      (candidate) =>
        candidate.start >= sections[index].start &&
        candidate.start < heading.start,
    ).length;
    const target =
      section?.querySelectorAll(HEADING_SELECTOR)[position] ?? section;
    target?.scrollIntoView({ block: "start", behavior: "smooth" });
  }

  function first_text(block: Element): string {
    const walker = document.createTreeWalker(block, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const text = node.textContent?.replace(/\s+/g, " ").trim();
      if (text) {
        return text;
      }
    }
    return "";
  }

  function handle_dblclick(event: MouseEvent) {
    const target = event.target;
    if (
      !onedit ||
      !(target instanceof Element) ||
      target.closest(
        "a, button, input, label, [data-slot='scroll-area-scrollbar']",
      )
    ) {
      return;
    }
    const section = target.closest<HTMLElement>("[data-section-start]");
    const from = Number(section?.dataset.sectionStart ?? 0);
    const block = target.closest(BLOCK_SELECTOR);
    onedit(locate_rendered_text(md, block ? first_text(block) : "", from));
  }
</script>

{#snippet header()}
  <div
    class={cn(
      "flex min-h-9 items-center justify-between gap-2 border-b bg-background py-1",
      fill ? "shrink-0 ps-1 pe-4" : "sticky top-0 z-10 -mt-1 mb-3",
    )}
  >
    <p class="text-sm font-medium">{label}</p>
    {#if outline.length > 1}
      <MarkdownOutlineMenu
        {outline}
        onselect={(heading) => void go_to_heading(heading)}
      />
    {/if}
  </div>
{/snippet}

{#snippet body()}
  <!-- Double-click is a shortcut; the dialog's Edit button stays available. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={container}
    class={fill ? undefined : "[&_:is(h1,h2,h3,h4,h5,h6)]:scroll-mt-12"}
    title={onedit ? m.markdown_edit_hint() : undefined}
    ondblclick={handle_dblclick}
  >
    {#each sections as section, index (section.start)}
      <div data-section={index} data-section-start={section.start}>
        {#if section.heading && sections.length > 1}
          <Collapsible.Root
            open={!collapsed[index]}
            onOpenChange={(open) => set_collapsed(index, !open)}
          >
            <div class={["flex items-start gap-1", index > 0 && "mt-4"]}>
              <Collapsible.Trigger>
                {#snippet child({ props })}
                  <Button
                    {...props}
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    class="mt-0.5 shrink-0 text-muted-foreground"
                    aria-label={collapsed[index]
                      ? m.markdown_expand_section({
                          title: section.heading!.text,
                        })
                      : m.markdown_collapse_section({
                          title: section.heading!.text,
                        })}
                  >
                    <ChevronRightIcon
                      class={[
                        "transition-transform",
                        !collapsed[index] && "rotate-90",
                      ]}
                    />
                  </Button>
                {/snippet}
              </Collapsible.Trigger>
              <div class="min-w-0 flex-1">
                <Markdown
                  md={md.slice(section.heading.start, section.heading.end)}
                />
              </div>
            </div>
            <Collapsible.Content class="ps-7">
              <Markdown md={md.slice(section.heading.end, section.end)} />
            </Collapsible.Content>
          </Collapsible.Root>
        {:else}
          <Markdown md={md.slice(section.start, section.end)} />
        {/if}
      </div>
    {/each}
  </div>
{/snippet}

{#if fill}
  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    {@render header()}
    <ScrollArea orientation="vertical" class="min-h-0 flex-1">
      <div class="py-3 ps-1 pe-4">
        {@render body()}
      </div>
    </ScrollArea>
  </div>
{:else}
  {@render header()}
  {@render body()}
{/if}
