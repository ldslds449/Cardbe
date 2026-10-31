<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { tick, untrack, type ComponentProps } from "svelte";
  import EditorControls from "./editor-controls.svelte";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { cn } from "#lib/utils.js";
  import RichTextSurface from "./rich-text-surface.svelte";
  import PlainTextSurface from "./plain-text-surface.svelte";
  import { editorMode } from "./mode";
  import Markdown from "../../../routes/components/markdown.svelte";
  import type { RichTextEditorHandle } from "./rich-text-surface.svelte";

  let {
    ref = $bindable<RichTextEditorHandle | null>(null),
    value = $bindable(""),
    containerClass,
    onvaluechange,
    previewing = $bindable(false),
    splitPreview = $bindable(),
    language = $bindable<string | null>(null),
    showControls = true,
    ...props
  }: ComponentProps<typeof RichTextSurface> & {
    previewing?: boolean;
    /** Plain-text side-by-side preview, rendered by the caller. */
    splitPreview?: boolean;
    showControls?: boolean;
  } = $props();
  let container = $state<HTMLDivElement | null>(null);
  let richRef = $state<RichTextEditorHandle | null>(null);
  let plainRef = $state<RichTextEditorHandle | null>(null);

  $effect(() => {
    ref = $editorMode === "rich" ? richRef : plainRef;
  });

  // Switching to rich mode also resets `previewing`. Keep the position
  // measured before the first change instead of the new editor's top.
  let restoringScroll = false;

  $effect.pre(() => {
    void $editorMode;
    void previewing;
    untrack(() => {
      const root = container;
      const body = root?.querySelector<HTMLElement>(
        "[data-slot='scroll-area-viewport']",
      );
      if (!root || !body || restoringScroll) {
        return;
      }
      restoringScroll = true;
      const scrollRatio =
        body.scrollTop / Math.max(1, body.scrollHeight - body.clientHeight);
      const parents: [HTMLElement, number, number][] = [];
      for (
        let element = root.parentElement;
        element;
        element = element.parentElement
      ) {
        parents.push([element, element.scrollTop, element.scrollLeft]);
      }
      void tick().then(() => {
        restoringScroll = false;
        if (!root.isConnected) {
          return;
        }
        const next = root.querySelector<HTMLElement>(
          "[data-slot='scroll-area-viewport']",
        );
        if (next) {
          next.scrollTop =
            scrollRatio * Math.max(0, next.scrollHeight - next.clientHeight);
        }
        for (const [element, top, left] of parents) {
          element.scrollTo({ top, left, behavior: "instant" });
        }
      });
    });
  });

  // Switching modes from this editor's controls keeps the cursor at the same
  // place in the Markdown source. The controls hold focus when they switch.
  let shownMode = untrack(() => $editorMode);
  $effect.pre(() => {
    const mode = $editorMode;
    untrack(() => {
      const previous = shownMode === "rich" ? richRef : plainRef;
      if (mode === shownMode) {
        return;
      }
      shownMode = mode;
      if (!container?.contains(document.activeElement)) {
        return;
      }
      const offset = previous?.getSourceOffset?.();
      if (offset === undefined) {
        return;
      }
      void tick().then(() =>
        (mode === "rich" ? richRef : plainRef)?.setSourceOffset?.(offset),
      );
    });
  });

  $effect(() => {
    if ($editorMode === "rich") {
      previewing = false;
    } else {
      language = null;
    }
  });
</script>

<div
  bind:this={container}
  class={cn("flex min-h-0 min-w-0 max-w-full flex-col", containerClass)}
>
  {#if showControls}
    <div class="shrink-0 border-b px-2 py-1">
      <EditorControls
        bind:previewing
        bind:splitPreview
        {language}
        disabled={props.disabled || props.readonly}
        onlanguagechange={(language) => ref?.setCodeLanguage?.(language)}
      />
    </div>
  {/if}
  {#if $editorMode === "rich"}
    <RichTextSurface
      {...props}
      containerClass="flex min-h-0 min-w-0 flex-1 flex-col"
      bind:ref={richRef}
      bind:value
      bind:language
      {onvaluechange}
    />
  {:else if previewing}
    <ScrollArea
      type="auto"
      class={cn(
        "min-h-0 min-w-0 max-w-full [overflow-wrap:anywhere]",
        props.class,
        "editor-scroll-area overflow-hidden",
      )}
      role="region"
      data-editor-body
      aria-label={props["aria-label"]
        ? m.editor_named_preview({ name: props["aria-label"] })
        : m.editor_preview()}
    >
      {#if value.trim()}
        <Markdown md={value} />
      {:else}
        <p class="text-sm text-muted-foreground">{m.editor_preview_empty()}</p>
      {/if}
    </ScrollArea>
  {:else}
    <ScrollArea
      type="auto"
      style={props.style}
      class={cn(
        props.class,
        "editor-scroll-area overflow-hidden rounded-none p-0 sm:p-0",
      )}
    >
      <PlainTextSurface
        bind:ref={plainRef}
        bind:value
        class={props.class}
        id={props.id}
        placeholder={props.placeholder}
        disabled={props.disabled}
        readonly={props.readonly}
        aria-label={props["aria-label"]}
        aria-describedby={props["aria-describedby"]}
        {onvaluechange}
      />
    </ScrollArea>
  {/if}
</div>
