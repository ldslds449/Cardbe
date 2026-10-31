<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { untrack } from "svelte";
  import "./markdown-content.css";
  import type { Editor } from "@tiptap/core";
  import { Editor as TiptapEditor } from "@tiptap/core";
  import { Selection, TextSelection, type Transaction } from "@tiptap/pm/state";
  import {
    count_occurrences,
    text_target_prefixes,
    type TextTarget,
  } from "../../../routes/utils/markdown-outline";
  import { cn } from "#lib/utils.js";
  import type { CardReferenceOption } from "./card-reference-picker.svelte";
  import { exitSuggestion, type SuggestionProps } from "@tiptap/suggestion";
  import { CardReferenceSuggestionKey } from "./extensions/card-reference";
  import CardReferencePicker from "./card-reference-picker.svelte";
  import { filterSlashCommands } from "./commands/slashCommands";
  import { createEditorExtensions } from "./extensions";
  import EditorToolbar from "./menus/editor-toolbar.svelte";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Popover from "#lib/components/ui/popover/index.js";
  import {
    markdownInsertionContent,
    serializeMarkdown,
  } from "./serialization/markdown";
  import {
    documentPosition,
    sourceOffset,
    textPosition,
  } from "./source-position";

  export type EditorSelection = { from: number; to: number };

  export type RichTextEditorHandle = {
    focus: () => void;
    getSelection: () => EditorSelection;
    getCursorRect?: () => DOMRect;
    getTextBeforeCursor: (length: number) => string;
    replaceMarkdown: (markdown: string, range?: EditorSelection) => void;
    setCodeLanguage?: (language: string | null) => void;
    /** Restores a selection from another editor in the same mode. */
    setSelection?: (selection: EditorSelection) => void;
    /** Focuses and scrolls to text located at a Markdown source offset. */
    reveal?: (target: TextTarget) => void;
    /** The cursor as a Markdown source offset, comparable across modes. */
    getSourceOffset?: () => number;
    /** Focuses and places the cursor at a Markdown source offset. */
    setSourceOffset?: (offset: number) => void;
  };

  type CursorAnchor = { getBoundingClientRect: () => DOMRect };
  // Menus are popovers anchored at the cursor: inside a transformed dialog, a
  // fixed menu would be offset, and scroll areas would clip it.
  type SlashState = EditorSelection & {
    query: string;
    anchor: CursorAnchor;
  };
  type ReferenceState = EditorSelection & {
    query: string;
    anchor: CursorAnchor;
  };

  let {
    ref = $bindable<RichTextEditorHandle | null>(null),
    value = $bindable(""),
    class: className,
    containerClass,
    showToolbar = false,
    language = $bindable<string | null>(null),
    placeholder = m.editor_placeholder(),
    id,
    style,
    disabled = false,
    readonly = false,
    "aria-label": ariaLabel,
    "aria-describedby": ariaDescribedby,
    cardReferenceOptions = [],
    cardReferenceExcludeId = "",
    onvaluechange,
  }: {
    ref?: RichTextEditorHandle | null;
    value?: string;
    class?: string;
    containerClass?: string;
    showToolbar?: boolean;
    language?: string | null;
    placeholder?: string;
    id?: string;
    style?: string;
    disabled?: boolean;
    readonly?: boolean;
    "aria-label"?: string;
    "aria-describedby"?: string;
    /** Cards offered after typing `[[`. */
    cardReferenceOptions?: CardReferenceOption[];
    cardReferenceExcludeId?: string;
    onvaluechange?: (value: string) => void;
  } = $props();

  let editor_host = $state<HTMLDivElement | null>(null);
  let editor = $state<Editor | null>(null);
  let slash = $state<SlashState | null>(null);
  let slashIndex = $state(0);
  let reference = $state<ReferenceState | null>(null);
  let referenceOpen = $state(false);
  let revision = $state(0);
  let lastValue = value;
  // Markdown re-serialization can rewrite equivalent input (for example bare
  // URLs become `[url](url)`). Keep the caller's text until content changes.
  let baselineValue = value;
  let baselineMarkdown: string | null = null;

  const visibleSlashCommands = $derived(
    slash ? filterSlashCommands(slash.query) : [],
  );

  $effect(() => {
    slashIndex = Math.min(
      slashIndex,
      Math.max(0, visibleSlashCommands.length - 1),
    );
  });

  $effect(() => {
    void revision;
    language = editor?.isActive("codeBlock")
      ? (editor.getAttributes("codeBlock").language ?? "")
      : null;
  });

  function editorSelection(instance: Editor): EditorSelection {
    return {
      from: instance.state.selection.from,
      to: instance.state.selection.to,
    };
  }

  function cursorAnchor(instance: Editor, position: number): CursorAnchor {
    return {
      getBoundingClientRect: () => {
        const { left, top, bottom } = instance.view.coordsAtPos(
          Math.min(position, instance.state.doc.content.size),
        );
        return new DOMRect(left, top, 0, bottom - top);
      },
    };
  }

  function updateSlashMenu(instance: Editor) {
    const selection = instance.state.selection;
    if (
      !instance.isFocused ||
      !instance.isEditable ||
      !selection.empty ||
      !selection.$from.parent.isTextblock ||
      selection.$from.parent.type.spec.code
    ) {
      slash = null;
      return;
    }

    const before = selection.$from.parent.textBetween(
      0,
      selection.$from.parentOffset,
      "\n",
      "\ufffc",
    );
    // A space right after the slash dismisses the menu to type a plain `/`.
    const match = /(?:^|\s)\/((?:\S[^\n]*)?)$/.exec(before);
    if (!match) {
      slash = null;
      return;
    }

    const from = selection.from - match[1].length - 1;
    const next = {
      from,
      to: selection.to,
      query: match[1].trimStart(),
      anchor: cursorAnchor(instance, from),
    };
    if (!slash || slash.query !== next.query) {
      slashIndex = 0;
    }
    slash = next;
  }

  function runSlashCommand(index: number) {
    const instance = editor;
    const command = visibleSlashCommands[index];
    const range = slash;
    if (!instance || !command || !range) {
      return;
    }

    slash = null;
    instance.chain().focus().deleteRange(range).run();
    command.action(instance);
  }

  function handleEditorKeydown(_view: unknown, event: KeyboardEvent): boolean {
    if (!slash || event.isComposing || visibleSlashCommands.length === 0) {
      return false;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      slashIndex = Math.min(slashIndex + 1, visibleSlashCommands.length - 1);
      return true;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      slashIndex = Math.max(slashIndex - 1, 0);
      return true;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      runSlashCommand(slashIndex);
      return true;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      slash = null;
      return true;
    }
    return false;
  }

  function showReference({
    editor: instance,
    range,
    query,
    clientRect,
  }: SuggestionProps) {
    referenceOpen = true;
    reference = {
      ...range,
      query,
      anchor: {
        getBoundingClientRect: () => {
          const rect = clientRect?.();
          if (rect) {
            return rect;
          }
          const { left, top, bottom } = instance.view.coordsAtPos(range.from);
          return new DOMRect(left, top, 0, bottom - top);
        },
      },
    };
  }

  function revealPosition(instance: Editor, target: TextTarget): number {
    for (const prefix of text_target_prefixes(target.text)) {
      const position = textPosition(
        instance.state.doc,
        prefix,
        count_occurrences(value, prefix, target.offset),
      );
      if (position !== null) {
        return position;
      }
    }
    // Markdown offsets do not map to document positions; approximate.
    const ratio = target.offset / Math.max(1, value.length);
    return Math.round(ratio * instance.state.doc.content.size);
  }

  // ProseMirror only scrolls to a selection the DOM already shows. An
  // unfocused editor, such as one in a dialog that just opened, gets focus on
  // the next frame, so Tiptap scrolls again after focusing it.
  function moveCursor(
    instance: Editor,
    select: (tr: Transaction) => Selection,
  ) {
    instance
      .chain()
      .focus(undefined, { scrollIntoView: true })
      .command(({ tr }) => {
        tr.setSelection(select(tr));
        return true;
      })
      .scrollIntoView()
      .run();
    settleScroll(instance, 8);
  }

  // Offscreen blocks have estimated heights (content-visibility), so a long
  // jump can land short once the blocks it reaches are laid out. Scroll again
  // until the cursor stays in view.
  function settleScroll(instance: Editor, frames: number) {
    requestAnimationFrame(() => {
      if (instance.isDestroyed || frames === 0) {
        return;
      }
      const { view } = instance;
      const viewport = view.dom
        .closest("[data-slot='scroll-area-viewport']")
        ?.getBoundingClientRect();
      const cursor = view.coordsAtPos(view.state.selection.head);
      if (
        viewport &&
        (cursor.top < viewport.top || cursor.bottom > viewport.bottom)
      ) {
        instance.commands.scrollIntoView();
        settleScroll(instance, frames - 1);
      }
    });
  }

  function createHandle(instance: Editor): RichTextEditorHandle {
    return {
      setSelection: ({ from, to }) => {
        const size = instance.state.doc.content.size;
        moveCursor(instance, (tr) =>
          TextSelection.between(
            tr.doc.resolve(Math.min(from, size)),
            tr.doc.resolve(Math.min(to, size)),
          ),
        );
      },
      reveal: (target) => {
        const position = Math.min(
          revealPosition(instance, target),
          instance.state.doc.content.size,
        );
        moveCursor(instance, (tr) => Selection.near(tr.doc.resolve(position)));
      },
      getSourceOffset: () =>
        sourceOffset(instance.state.doc, value, instance.state.selection.head),
      setSourceOffset: (offset) => {
        const position = Math.min(
          documentPosition(instance.state.doc, value, offset),
          instance.state.doc.content.size,
        );
        moveCursor(instance, (tr) => Selection.near(tr.doc.resolve(position)));
      },
      focus: () =>
        instance.commands.focus(undefined, { scrollIntoView: false }),
      getSelection: () => editorSelection(instance),
      setCodeLanguage: (language) => {
        instance
          .chain()
          .focus(undefined, { scrollIntoView: false })
          .updateAttributes("codeBlock", { language })
          .run();
      },
      getTextBeforeCursor: (length) => {
        const { from } = instance.state.selection;
        return instance.state.doc.textBetween(
          Math.max(0, from - length),
          from,
          "\n",
          "\ufffc",
        );
      },
      replaceMarkdown: (markdown, range = editorSelection(instance)) => {
        instance
          .chain()
          .focus()
          .insertContentAt(range, markdownInsertionContent(instance, markdown))
          .run();
      },
    };
  }

  // Wait for the host instead of using onMount: another editor's code block
  // view, or creating the first of two editors mounted during a mode switch,
  // can flush this component's mount before `bind:this` is assigned.
  $effect(() => {
    const host = editor_host;
    if (host) {
      return untrack(() => createEditor(host));
    }
  });

  function createEditor(host: HTMLDivElement) {
    const instance = new TiptapEditor({
      element: host,
      extensions: createEditorExtensions(placeholder, {
        render: () => ({
          onStart: showReference,
          onUpdate: showReference,
          onExit: () => {
            referenceOpen = false;
            reference = null;
          },
        }),
      }),
      content: value,
      contentType: "markdown",
      editable: !disabled && !readonly,
      injectCSS: false,
      editorProps: {
        attributes: {
          role: "textbox",
          "aria-multiline": "true",
          ...(ariaLabel ? { "aria-label": ariaLabel } : {}),
          ...(ariaDescribedby ? { "aria-describedby": ariaDescribedby } : {}),
        },
        handleKeyDown: handleEditorKeydown,
      },
      onUpdate: ({ editor: updatedEditor }) => {
        const serialized = serializeMarkdown(updatedEditor);
        const nextValue =
          serialized === baselineMarkdown ? baselineValue : serialized;
        lastValue = nextValue;
        value = nextValue;
        onvaluechange?.(nextValue);
        updateSlashMenu(updatedEditor);
      },
      onSelectionUpdate: ({ editor: updatedEditor }) => {
        updateSlashMenu(updatedEditor);
      },
      onTransaction: () => {
        revision += 1;
      },
      onFocus: ({ editor: focusedEditor }) => updateSlashMenu(focusedEditor),
      onBlur: () => {
        slash = null;
      },
    });

    baselineValue = value;
    lastValue = value;
    baselineMarkdown = serializeMarkdown(instance);
    editor = instance;
    const handle = createHandle(instance);
    ref = handle;
    updateSlashMenu(instance);

    return () => {
      if (ref?.focus === handle.focus) {
        ref = null;
      }
      instance.destroy();
      editor = null;
    };
  }

  $effect(() => {
    const instance = editor;
    if (instance) {
      const editable = !disabled && !readonly;
      // An update event would report unchanged content as an edit.
      untrack(() => {
        instance.setEditable(editable, false);
        revision += 1;
      });
    }
  });

  $effect(() => {
    const instance = editor;
    if (!instance || value === lastValue) {
      return;
    }
    const content = value;
    lastValue = content;
    untrack(() => {
      instance.commands.setContent(content, {
        contentType: "markdown",
        emitUpdate: false,
      });
      baselineValue = content;
      baselineMarkdown = serializeMarkdown(instance);
      updateSlashMenu(instance);
    });
  });
</script>

<svelte:window
  onresize={() => slash && editor && updateSlashMenu(editor)}
  onscrollcapture={() => slash && editor && updateSlashMenu(editor)}
/>

<div class={cn("relative min-w-0 w-full max-w-full", containerClass)}>
  {#if showToolbar}
    <EditorToolbar {editor} {revision} />
  {/if}

  <ScrollArea
    type="auto"
    {style}
    class={cn(className, "editor-scroll-area overflow-hidden")}
  >
    <div
      bind:this={editor_host}
      data-editor-body
      {id}
      class="markdown-content min-h-full"
      aria-disabled={disabled || undefined}
    ></div>
  </ScrollArea>

  {#if slash && visibleSlashCommands.length > 0}
    <Popover.Root
      open
      onOpenChange={(open) => {
        if (!open) {
          slash = null;
        }
      }}
    >
      <!-- The editor keeps focus and handles the menu keys. -->
      <Popover.Content
        customAnchor={slash.anchor}
        side="bottom"
        align="start"
        sideOffset={6}
        trapFocus={false}
        interactOutsideBehavior="ignore"
        escapeKeydownBehavior="ignore"
        onOpenAutoFocus={(event) => event.preventDefault()}
        onCloseAutoFocus={(event) => event.preventDefault()}
        class="w-[min(17.5rem,calc(100vw-1rem))] overflow-hidden p-0"
      >
        <ScrollArea
          class="max-h-[min(20rem,var(--bits-popover-content-available-height))] [&>[data-slot=scroll-area-viewport]]:max-h-[inherit]"
        >
          <div
            role="listbox"
            aria-label={m.editor_slash_commands()}
            class="p-1"
            {@attach (listbox) => {
              // Keep the keyboard-selected command visible.
              listbox.children[slashIndex]?.scrollIntoView({
                block: "nearest",
              });
            }}
          >
            {#each visibleSlashCommands as command, index (command.id)}
              <Button
                type="button"
                variant={index === slashIndex ? "secondary" : "ghost"}
                role="option"
                aria-selected={index === slashIndex}
                class="h-auto w-full items-start justify-start whitespace-normal px-2 py-1.5 text-left"
                onpointerdown={(event) => event.preventDefault()}
                onclick={() => runSlashCommand(index)}
              >
                <span class="min-w-0">
                  <span class="block font-medium">{command.label}</span>
                  <span class="block truncate text-xs text-muted-foreground"
                    >{command.description}</span
                  >
                </span>
              </Button>
            {/each}
          </div>
        </ScrollArea>
      </Popover.Content>
    </Popover.Root>
  {/if}

  <CardReferencePicker
    inline
    editor={ref}
    options={cardReferenceOptions}
    excludeTaskId={cardReferenceExcludeId}
    bind:open={referenceOpen}
    triggerStart={reference?.from}
    anchor={reference?.anchor}
    query={reference?.query ?? ""}
    onclose={() => {
      if (editor) {
        exitSuggestion(editor.view, CardReferenceSuggestionKey);
      }
    }}
  />
</div>

<style>
  :global(.ProseMirror) {
    min-height: 100%;
    width: 100%;
    overflow-wrap: anywhere;
    outline: none;
    white-space: pre-wrap;
  }

  /* Long descriptions with many diagrams spend most of each keystroke laying
     out and committing offscreen blocks; let the browser skip them. Paint
     containment would clip focus rings and selection outlines at block edges.
     Containment also stops child margins from collapsing through the block:
     lists keep nested margins at their edges, so they are left out. */
  :global(.ProseMirror > :not(ul, ol, blockquote, .ProseMirror-gapcursor)) {
    content-visibility: auto;
    contain-intrinsic-size: auto 3rem;
    overflow-clip-margin: 0.5rem;
  }

  /* Code and math node views wrap a child that carries the block margin. */
  :global(.ProseMirror > div:not(.tableWrapper)) {
    margin-block: 0.75rem;
  }

  :global(.ProseMirror > div:not(.tableWrapper) > :first-child) {
    margin-top: 0;
  }

  :global(.ProseMirror > div:not(.tableWrapper) > :last-child) {
    margin-bottom: 0;
  }

  :global(.ProseMirror p.is-editor-empty:first-child::before) {
    float: left;
    height: 0;
    color: var(--muted-foreground);
    pointer-events: none;
    content: attr(data-placeholder);
  }
</style>
