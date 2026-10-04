<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { mode } from "mode-watcher";
  import "./markdown-content.css";
  import type { Editor } from "@tiptap/core";
  import { Editor as TiptapEditor } from "@tiptap/core";
  import { cn } from "$lib/utils.js";
  import { filterSlashCommands } from "./commands/slashCommands";
  import { createEditorExtensions } from "./extensions";
  import { codeHighlightKey } from "./extensions/code-highlight";
  import EditorToolbar from "./menus/editor-toolbar.svelte";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import {
    markdownInsertionContent,
    serializeMarkdown,
  } from "./serialization/markdown";

  export type EditorSelection = { from: number; to: number };

  export type RichTextEditorHandle = {
    focus: () => void;
    getSelection: () => EditorSelection;
    getTextBeforeCursor: (length: number) => string;
    replaceMarkdown: (markdown: string, range?: EditorSelection) => void;
    setCodeLanguage?: (language: string | null) => void;
  };

  type MenuPosition = { left: number; top: number };
  type SlashState = EditorSelection & {
    query: string;
    position: MenuPosition;
  };

  let {
    ref = $bindable<RichTextEditorHandle | null>(null),
    value = $bindable(""),
    class: className,
    containerClass,
    showToolbar = false,
    language = $bindable<string | null>(null),
    placeholder = "Start writing…",
    id,
    style,
    disabled = false,
    readonly = false,
    "aria-label": ariaLabel,
    "aria-describedby": ariaDescribedby,
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
    onvaluechange?: (value: string) => void;
  } = $props();

  let editor_host = $state<HTMLDivElement | null>(null);
  let editor = $state<Editor | null>(null);
  let slash = $state<SlashState | null>(null);
  let slashIndex = $state(0);
  let revision = $state(0);
  let lastValue = value;

  const visibleSlashCommands = $derived(
    slash ? filterSlashCommands(slash.query) : [],
  );

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

  function menuPosition(instance: Editor, position: number): MenuPosition {
    const rect = instance.view.coordsAtPos(position);
    const menuWidth = 280;
    const menuHeight = 320;
    return {
      left: Math.max(
        8,
        Math.min(rect.left, Math.max(8, window.innerWidth - menuWidth - 8)),
      ),
      top: Math.max(
        8,
        Math.min(rect.bottom + 6, window.innerHeight - menuHeight - 8),
      ),
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
    const match = /(?:^|\s)\/([^\n]*)$/.exec(before);
    if (!match) {
      slash = null;
      return;
    }

    const from = selection.from - match[1].length - 1;
    const next = {
      from,
      to: selection.to,
      query: match[1].trimStart(),
      position: menuPosition(instance, from),
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

  function createHandle(instance: Editor): RichTextEditorHandle {
    return {
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

  onMount(() => {
    if (!editor_host) {
      return;
    }

    const instance = new TiptapEditor({
      element: editor_host,
      extensions: createEditorExtensions(placeholder),
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
        const nextValue = serializeMarkdown(updatedEditor);
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
  });

  $effect(() => {
    void mode.current;
    const instance = editor;
    if (instance) {
      untrack(() =>
        instance.view.dispatch(
          instance.state.tr.setMeta(codeHighlightKey, true),
        ),
      );
    }
  });

  $effect(() => {
    const instance = editor;
    if (instance) {
      const editable = !disabled && !readonly;
      untrack(() => instance.setEditable(editable));
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
    <div
      role="listbox"
      aria-label="Block commands"
      class="fixed z-50 max-h-[min(20rem,calc(100vh-1rem))] w-[min(17.5rem,calc(100vw-1rem))] overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
      style:left={`${slash.position.left}px`}
      style:top={`${slash.position.top}px`}
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
  {/if}
</div>

<style>
  :global(.ProseMirror) {
    min-height: 100%;
    width: 100%;
    overflow-wrap: anywhere;
    outline: none;
    white-space: pre-wrap;
  }

  :global(.ProseMirror p.is-editor-empty:first-child::before) {
    float: left;
    height: 0;
    color: var(--muted-foreground);
    pointer-events: none;
    content: attr(data-placeholder);
  }
</style>
