<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { untrack } from "svelte";
  import { Compartment, EditorState } from "@codemirror/state";
  import {
    EditorView,
    keymap,
    placeholder as placeholderText,
  } from "@codemirror/view";
  import {
    defaultKeymap,
    history,
    historyKeymap,
    insertNewline,
  } from "@codemirror/commands";
  import { cn } from "#lib/utils.js";
  import type { RichTextEditorHandle } from "./rich-text-surface.svelte";

  let {
    ref = $bindable<RichTextEditorHandle | null>(null),
    value = $bindable(""),
    class: className,
    placeholder,
    id,
    disabled = false,
    readonly = false,
    "aria-label": ariaLabel,
    "aria-describedby": ariaDescribedby,
    onvaluechange,
  }: {
    ref?: RichTextEditorHandle | null;
    value?: string;
    class?: string;
    placeholder?: string;
    id?: string;
    disabled?: boolean;
    readonly?: boolean;
    "aria-label"?: string;
    "aria-describedby"?: string;
    onvaluechange?: (value: string) => void;
  } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let view = $state.raw<EditorView | null>(null);
  let lastValue = value;
  const settings = new Compartment();

  // Keys behave as in the former textarea: Enter only breaks the line, and
  // Ctrl+Enter submits the surrounding dialog.
  const keys = [
    { key: "Enter", run: insertNewline },
    ...defaultKeymap.filter((binding) => binding.key !== "Mod-Enter"),
  ];

  // Text inherits the caller's typography; the enclosing ScrollArea scrolls,
  // and CodeMirror only renders the lines near that viewport.
  const theme = EditorView.theme({
    "&": { flex: "1 1 auto" },
    "&.cm-focused": { outline: "none" },
    // Stretch the content so clicks below the text still place the cursor.
    ".cm-scroller": {
      flex: "1 1 auto",
      alignItems: "stretch",
      overflow: "visible",
      fontFamily: "inherit",
      lineHeight: "inherit",
    },
    // The default light theme draws a black caret, invisible on dark themes.
    ".cm-content": { padding: "0", caretColor: "currentColor" },
    ".cm-line": { padding: "0" },
    ".cm-placeholder": { color: "var(--muted-foreground)" },
  });

  function configure() {
    return [
      EditorView.editable.of(!disabled),
      EditorState.readOnly.of(readonly),
      EditorView.contentAttributes.of({
        ...(id ? { id } : {}),
        "aria-label": ariaLabel ?? m.editor_markdown_content(),
        ...(ariaDescribedby ? { "aria-describedby": ariaDescribedby } : {}),
        ...(disabled ? { "aria-disabled": "true" } : {}),
        spellcheck: "true",
      }),
      placeholder ? placeholderText(placeholder) : [],
    ];
  }

  function createState(doc: string) {
    return EditorState.create({
      doc,
      extensions: [
        history(),
        keymap.of([...keys, ...historyKeymap]),
        EditorView.lineWrapping,
        theme,
        settings.of(configure()),
        EditorView.updateListener.of((update) => {
          if (!update.docChanged) {
            return;
          }
          lastValue = update.state.doc.toString();
          value = lastValue;
          onvaluechange?.(lastValue);
        }),
      ],
    });
  }

  $effect(() => {
    const parent = host;
    if (!parent) {
      return;
    }
    const created = new EditorView({
      parent,
      state: createState(untrack(() => value)),
    });
    lastValue = untrack(() => value);
    view = created;
    // The padding belonged to the textarea; clicking it still edits.
    const focusFromPadding = (event: MouseEvent) => {
      if (
        event.target !== parent ||
        !created.state.facet(EditorView.editable)
      ) {
        return;
      }
      event.preventDefault();
      created.dispatch({
        selection: {
          anchor: created.posAtCoords(
            { x: event.clientX, y: event.clientY },
            false,
          ),
        },
      });
      created.focus();
    };
    parent.addEventListener("mousedown", focusFromPadding);
    return () => {
      parent.removeEventListener("mousedown", focusFromPadding);
      view = null;
      created.destroy();
    };
  });

  // Text replaced by the caller starts a new undo history, like a textarea.
  $effect(() => {
    const next = value;
    if (view && next !== lastValue) {
      lastValue = next;
      view.setState(createState(next));
    }
  });

  $effect(() => {
    view?.dispatch({ effects: settings.reconfigure(configure()) });
  });

  $effect(() => {
    const current = view;
    if (!current) {
      return;
    }
    const select = (from: number, to = from) => {
      const length = current.state.doc.length;
      current.dispatch({
        selection: {
          anchor: Math.min(from, length),
          head: Math.min(to, length),
        },
        effects: EditorView.scrollIntoView(Math.min(from, length), {
          y: "nearest",
          // The enclosing ScrollArea scrolls, not CodeMirror's own scroller.
          yMargin:
            (current.dom.closest("[data-slot='scroll-area-viewport']")
              ?.clientHeight ?? 0) / 3,
        }),
      });
      // Like Tiptap, focus after a closing menu has restored its own focus.
      requestAnimationFrame(() => current.focus());
    };
    const handle: RichTextEditorHandle = {
      focus: () => current.focus(),
      getSelection: () => {
        const { from, to } = current.state.selection.main;
        return { from, to };
      },
      getCursorRect: () => {
        const coords = current.coordsAtPos(current.state.selection.main.head);
        return coords
          ? new DOMRect(coords.left, coords.top, 0, coords.bottom - coords.top)
          : current.contentDOM.getBoundingClientRect();
      },
      getTextBeforeCursor: (length) => {
        const head = current.state.selection.main.head;
        return current.state.sliceDoc(Math.max(0, head - length), head);
      },
      setSelection: ({ from, to }) => select(from, to),
      reveal: ({ offset }) => select(offset),
      getSourceOffset: () => current.state.selection.main.head,
      setSourceOffset: (offset) => select(offset),
      replaceMarkdown: (markdown, range = current.state.selection.main) => {
        current.dispatch({
          changes: { from: range.from, to: range.to, insert: markdown },
          selection: { anchor: range.from + markdown.length },
          scrollIntoView: true,
          userEvent: "input",
        });
      },
    };
    ref = handle;
    return () => {
      ref = null;
    };
  });
</script>

<div
  bind:this={host}
  data-editor-body
  class={cn(
    "plain-text-editor flex flex-col text-base md:text-sm",
    className,
    "h-auto min-h-full max-h-none flex-none rounded-none border-0 bg-transparent shadow-none",
    disabled && "cursor-not-allowed opacity-50",
  )}
></div>
