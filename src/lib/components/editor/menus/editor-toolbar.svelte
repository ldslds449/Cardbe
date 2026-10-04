<script lang="ts">
  import type { Editor } from "@tiptap/core";
  import BoldIcon from "@lucide/svelte/icons/bold";
  import CodeIcon from "@lucide/svelte/icons/code";
  import Code2Icon from "@lucide/svelte/icons/code-2";
  import Heading1Icon from "@lucide/svelte/icons/heading-1";
  import Heading2Icon from "@lucide/svelte/icons/heading-2";
  import Heading3Icon from "@lucide/svelte/icons/heading-3";
  import ItalicIcon from "@lucide/svelte/icons/italic";
  import Link2Icon from "@lucide/svelte/icons/link-2";
  import ListIcon from "@lucide/svelte/icons/list";
  import ListChecksIcon from "@lucide/svelte/icons/list-checks";
  import ListOrderedIcon from "@lucide/svelte/icons/list-ordered";
  import MinusIcon from "@lucide/svelte/icons/minus";
  import QuoteIcon from "@lucide/svelte/icons/quote";
  import Redo2Icon from "@lucide/svelte/icons/redo-2";
  import StrikethroughIcon from "@lucide/svelte/icons/strikethrough";
  import Undo2Icon from "@lucide/svelte/icons/undo-2";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { FieldError } from "$lib/components/ui/field/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";

  let { editor, revision }: { editor: Editor | null; revision: number } =
    $props();
  let linkOpen = $state(false);
  let linkHref = $state("");
  let linkError = $state("");
  let linkSelection = $state<{ from: number; to: number } | null>(null);
  function isActive(name: string, attributes?: Record<string, unknown>) {
    void revision;
    return editor?.isActive(name, attributes);
  }

  function canUndo() {
    void revision;
    return editor?.isEditable && editor.can().undo();
  }

  function canRedo() {
    void revision;
    return editor?.isEditable && editor.can().redo();
  }

  function toggleLink() {
    if (!editor?.isEditable) {
      return;
    }
    if (editor.isActive("link")) {
      editor.chain().focus().unsetLink().run();
      return;
    }

    linkSelection = {
      from: editor.state.selection.from,
      to: editor.state.selection.to,
    };
    linkHref = "";
    linkError = "";
    linkOpen = true;
  }

  function addLink(event: SubmitEvent) {
    event.preventDefault();
    event.stopPropagation();
    if (!editor?.isEditable || !linkSelection) {
      return;
    }
    const input = (event.currentTarget as HTMLFormElement).querySelector(
      "input",
    )!;
    if (!linkHref.trim()) {
      linkError = "Enter a link URL.";
      input.focus();
      return;
    }
    const linked = editor
      .chain()
      .setTextSelection(linkSelection)
      .setLink({ href: linkHref.trim() })
      .run();
    if (!linked) {
      linkError = "This URL is not allowed. Use a safe link URL.";
      input.focus();
      return;
    }
    linkOpen = false;
  }
</script>

<div
  class="sticky top-0 z-10 flex shrink-0 flex-wrap items-center gap-1 border-b bg-background/95 p-2 text-foreground shadow-sm backdrop-blur-sm [&_[aria-pressed=true]]:bg-primary [&_[aria-pressed=true]]:text-primary-foreground [&_[aria-pressed=true]:hover]:bg-primary/80"
  role="toolbar"
  aria-label="Text formatting"
  tabindex="-1"
  onpointerdown={(event) => event.preventDefault()}
>
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Undo"
    title="Undo"
    disabled={!canUndo()}
    onclick={() => editor?.chain().focus().undo().run()}><Undo2Icon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Redo"
    title="Redo"
    disabled={!canRedo()}
    onclick={() => editor?.chain().focus().redo().run()}><Redo2Icon /></Button
  >

  <Separator orientation="vertical" class="mx-2 h-6" />

  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Heading 1"
    title="Heading 1"
    aria-pressed={isActive("heading", { level: 1 })}
    onclick={() => editor?.chain().focus().toggleHeading({ level: 1 }).run()}
    ><Heading1Icon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Heading 2"
    title="Heading 2"
    aria-pressed={isActive("heading", { level: 2 })}
    onclick={() => editor?.chain().focus().toggleHeading({ level: 2 }).run()}
    ><Heading2Icon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Heading 3"
    title="Heading 3"
    aria-pressed={isActive("heading", { level: 3 })}
    onclick={() => editor?.chain().focus().toggleHeading({ level: 3 }).run()}
    ><Heading3Icon /></Button
  >

  <Separator orientation="vertical" class="mx-2 h-6" />

  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Bold"
    title="Bold"
    aria-pressed={isActive("bold")}
    onclick={() => editor?.chain().focus().toggleBold().run()}
    ><BoldIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Italic"
    title="Italic"
    aria-pressed={isActive("italic")}
    onclick={() => editor?.chain().focus().toggleItalic().run()}
    ><ItalicIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Strike"
    title="Strike"
    aria-pressed={isActive("strike")}
    onclick={() => editor?.chain().focus().toggleStrike().run()}
    ><StrikethroughIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Inline code"
    title="Inline code — format text within a sentence as code"
    aria-pressed={isActive("code")}
    onclick={() => editor?.chain().focus().toggleCode().run()}
    ><CodeIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label={isActive("link") ? "Remove link" : "Add link"}
    title={isActive("link") ? "Remove link" : "Add link"}
    aria-pressed={isActive("link")}
    onclick={toggleLink}><Link2Icon /></Button
  >

  <Separator orientation="vertical" class="mx-2 h-6" />

  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Bullet list"
    title="Bullet list"
    aria-pressed={isActive("bulletList")}
    onclick={() => editor?.chain().focus().toggleBulletList().run()}
    ><ListIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Numbered list"
    title="Numbered list"
    aria-pressed={isActive("orderedList")}
    onclick={() => editor?.chain().focus().toggleOrderedList().run()}
    ><ListOrderedIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Task list"
    title="Task list"
    aria-pressed={isActive("taskList")}
    onclick={() => editor?.chain().focus().toggleTaskList().run()}
    ><ListChecksIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Blockquote"
    title="Blockquote"
    aria-pressed={isActive("blockquote")}
    onclick={() => editor?.chain().focus().toggleBlockquote().run()}
    ><QuoteIcon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Code block"
    title="Code block"
    aria-pressed={isActive("codeBlock")}
    onclick={() => editor?.chain().focus().toggleCodeBlock().run()}
    ><Code2Icon /></Button
  >
  <Button
    type="button"
    variant="ghost"
    size="icon-sm"
    aria-label="Horizontal rule"
    title="Horizontal rule"
    onclick={() => editor?.chain().focus().setHorizontalRule().run()}
    ><MinusIcon /></Button
  >
</div>

<Dialog.Root bind:open={linkOpen}>
  <Dialog.Content
    onCloseAutoFocus={(event) => {
      event.preventDefault();
      editor?.commands.focus(undefined, { scrollIntoView: false });
    }}
  >
    <Dialog.Header>
      <Dialog.Title>Add link</Dialog.Title>
      <Dialog.Description>Enter a URL for the selected text.</Dialog.Description
      >
    </Dialog.Header>
    <form class="grid gap-4" novalidate autocomplete="off" onsubmit={addLink}>
      <div class="grid gap-2">
        <Label for="editor-link-url">Link URL</Label>
        <Input
          id="editor-link-url"
          type="text"
          autocomplete="off"
          bind:value={linkHref}
          placeholder="https://example.com"
          required
          aria-invalid={linkError ? true : undefined}
          aria-describedby={linkError ? "editor-link-error" : undefined}
          oninput={() => (linkError = "")}
        />
        {#if linkError}
          <FieldError id="editor-link-error">{linkError}</FieldError>
        {/if}
      </div>
      <Dialog.Footer>
        <Button
          type="button"
          variant="outline"
          onclick={() => (linkOpen = false)}>Cancel</Button
        >
        <Button type="submit">Add link</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
