<script lang="ts">
  import { tick, untrack, type ComponentProps } from "svelte";
  import EditorControls from "./editor-controls.svelte";
  import { Textarea } from "$lib/components/ui/textarea/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { cn } from "$lib/utils.js";
  import RichTextSurface from "./rich-text-surface.svelte";
  import { editorMode } from "./mode";
  import Markdown from "../../../routes/components/markdown.svelte";
  import type { RichTextEditorHandle } from "./rich-text-surface.svelte";

  let {
    ref = $bindable<RichTextEditorHandle | null>(null),
    value = $bindable(""),
    containerClass,
    onvaluechange,
    previewing = $bindable(false),
    language = $bindable<string | null>(null),
    showControls = true,
    ...props
  }: ComponentProps<typeof RichTextSurface> & {
    previewing?: boolean;
    showControls?: boolean;
  } = $props();
  let textarea = $state<HTMLTextAreaElement | null>(null);
  let container = $state<HTMLDivElement | null>(null);
  let richRef = $state<RichTextEditorHandle | null>(null);
  let plainRef = $state<RichTextEditorHandle | null>(null);

  $effect(() => {
    ref = $editorMode === "rich" ? richRef : plainRef;
  });

  $effect.pre(() => {
    void $editorMode;
    void previewing;
    untrack(() => {
      const root = container;
      const body = root?.querySelector<HTMLElement>(
        "[data-slot='scroll-area-viewport']",
      );
      if (!root || !body) {
        return;
      }
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

  $effect(() => {
    if ($editorMode !== "plain" || previewing || !textarea) {
      return;
    }
    const input = textarea;
    const handle: RichTextEditorHandle = {
      focus: () => input.focus({ preventScroll: true }),
      getSelection: () => ({
        from: input.selectionStart,
        to: input.selectionEnd,
      }),
      getTextBeforeCursor: (length) =>
        input.value.slice(
          Math.max(0, input.selectionStart - length),
          input.selectionStart,
        ),
      replaceMarkdown: (
        markdown,
        range = { from: input.selectionStart, to: input.selectionEnd },
      ) => {
        input.setRangeText(markdown, range.from, range.to, "end");
        value = input.value;
        onvaluechange?.(value);
      },
    };
    plainRef = handle;
    return () => {
      plainRef = null;
    };
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
      aria-label={`${props["aria-label"] ?? "Content"} preview`}
    >
      {#if value.trim()}
        <Markdown md={value} />
      {:else}
        <p class="text-sm text-muted-foreground">Nothing to preview yet.</p>
      {/if}
    </ScrollArea>
  {:else}
    <ScrollArea
      type="auto"
      style={props.style}
      class={cn(
        props.class,
        "editor-scroll-area overflow-hidden px-3 sm:px-3 focus-within:ring-2 focus-within:ring-ring/50",
      )}
    >
      <Textarea
        data-editor-body
        bind:ref={textarea}
        bind:value
        class={cn(
          props.class,
          "h-auto min-h-full flex-none resize-none overflow-hidden border-0 p-0 shadow-none focus-visible:ring-0 sm:p-0",
        )}
        id={props.id}
        placeholder={props.placeholder}
        disabled={props.disabled}
        readonly={props.readonly}
        aria-label={props["aria-label"] ?? "Markdown content"}
        aria-describedby={props["aria-describedby"]}
        oninput={(event) => {
          value = event.currentTarget.value;
          onvaluechange?.(value);
        }}
      />
    </ScrollArea>
  {/if}
</div>
