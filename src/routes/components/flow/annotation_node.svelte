<script lang="ts">
  import type { NodeProps } from "@xyflow/svelte";
  import type { AnnotationNode } from "./flow";
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import * as m from "#lib/paraglide/messages.js";
  import Markdown from "../markdown.svelte";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import CardSizeMenu from "./card_size_menu.svelte";
  import CardScaleHandle from "./card_scale_handle.svelte";
  import { onMount, tick, untrack } from "svelte";
  import { registerUnsavedChanges } from "#lib/unsaved-changes.js";
  import { Textarea } from "#lib/components/ui/textarea/index.js";
  let { id, data, selected }: NodeProps<AnnotationNode> = $props();
  let draft = $state("");
  let editor = $state<HTMLTextAreaElement | null>(null);
  let frame = $state<HTMLElement | null>(null);
  let committing = $state(false);
  let preview = $state<number | null>(null);
  let pending_commit = $state(false);
  const editing = $derived(
    data.editing_id === data.annotation.id && data.annotation.kind === "text",
  );
  onMount(() =>
    registerUnsavedChanges(
      () =>
        editing &&
        data.annotation.kind === "text" &&
        draft !== data.annotation.text,
    ),
  );
  onMount(() => {
    function outside_pointerdown(event: PointerEvent) {
      if (editing && frame && !event.composedPath().includes(frame)) {
        void commit();
      }
    }
    window.addEventListener("pointerdown", outside_pointerdown, {
      capture: true,
    });
    return () =>
      window.removeEventListener("pointerdown", outside_pointerdown, {
        capture: true,
      });
  });
  $effect(() => {
    if (editing && pending_commit && !data.read_only) {
      untrack(() => void commit());
    }
  });
  $effect(() => {
    if (editing) {
      untrack(() => {
        draft = data.annotation.kind === "text" ? data.annotation.text : "";
        void tick().then(() => editor?.focus());
      });
    }
  });
  async function commit() {
    if (!editing || committing) {
      return;
    }
    if (data.read_only) {
      pending_commit = true;
      return;
    }
    pending_commit = false;
    committing = true;
    try {
      await data.onSaveText(data.annotation.id, draft);
    } finally {
      committing = false;
    }
  }
</script>

<ContextMenu.Root>
  <ContextMenu.Trigger
    bind:ref={frame}
    style={`width: ${data.annotation.width}px; zoom: ${preview ?? data.annotation.scale ?? 1}`}
    tabindex={0}
    aria-haspopup="menu"
    class={`block w-full overflow-hidden text-foreground focus-visible:ring-2 focus-visible:ring-ring ${data.annotation.kind === "note" ? "rounded-sm border border-border bg-background pb-3 shadow-sm" : "p-3"} ${editing ? "bg-accent/50 outline-2 outline-dashed outline-primary outline-offset-2" : selected ? "ring-2 ring-ring" : ""}`}
    aria-label={data.annotation.kind === "text"
      ? m.flow_annotation()
      : m.flow_note_reference()}
    ondblclick={() => data.onEdit(data.annotation)}
    onkeydown={(event) => {
      if (!editing && event.key === "Enter") {
        event.preventDefault();
        event.stopPropagation();
        data.onEdit(data.annotation);
      }
    }}
  >
    {#if data.annotation.kind === "text"}
      {#if editing}
        <Textarea
          bind:ref={editor}
          bind:value={draft}
          maxlength={4000}
          readonly={data.read_only || committing}
          aria-label={m.flow_annotation()}
          placeholder={m.flow_annotation_empty()}
          class="nodrag nopan min-h-7 resize-none rounded-none border-0 bg-transparent p-0 text-base leading-7 caret-primary shadow-none focus-visible:ring-0 md:text-base dark:bg-transparent"
          onblur={commit}
          onkeydown={(event) => {
            event.stopPropagation();
            if (event.key === "Escape") {
              event.preventDefault();
              pending_commit = false;
              data.onCancelEdit();
            } else if (
              event.key === "Enter" &&
              (event.ctrlKey || event.metaKey)
            ) {
              event.preventDefault();
              void commit();
            }
          }}
        />
      {:else}
        <p class="whitespace-pre-wrap break-words text-base leading-7">
          {data.annotation.text || m.flow_annotation_empty()}
        </p>
      {/if}
    {:else}
      <div
        class="flex items-center gap-2 border-b bg-muted/40 px-3 py-1.5 text-xs font-medium text-muted-foreground"
      >
        <FileTextIcon class="size-3.5" />
        {m.flow_note_reference()}
      </div>
      <p
        class="px-3 pt-2.5 line-clamp-2 break-words font-semibold text-base leading-6"
      >
        {data.note
          ? data.note.title.trim() || m.note_untitled()
          : data.notes_unavailable
            ? m.flow_note_unavailable()
            : m.error_note_not_found()}
      </p>
      {#if data.note}
        <div
          class="nodrag nopan mx-3 mt-2 max-h-24 overflow-hidden border-l-2 border-border pl-3 break-words text-sm leading-6 text-muted-foreground"
        >
          <Markdown
            md={Array.from(data.note.content).slice(0, 500).join("") ||
              m.note_empty()}
            compact
          />
        </div>
      {/if}
    {/if}
  </ContextMenu.Trigger>
  <ContextMenu.Content>
    <!-- Menus ignore pending saves so they stay steady while stepping sizes. -->
    {#if !data.locked}
      <CardSizeMenu
        scale={data.annotation.scale ?? 1}
        onScale={(change) =>
          data.onScale(`annotation_${data.annotation.id}`, change, true)}
      />
      <ContextMenu.Item
        onclick={() => data.onRemove(`annotation_${data.annotation.id}`)}
        >{m.flow_remove_annotation()}</ContextMenu.Item
      >
    {/if}
  </ContextMenu.Content>
</ContextMenu.Root>
{#if selected && !data.locked}
  <CardScaleHandle
    {id}
    scale={data.annotation.scale ?? 1}
    target={frame}
    base_width={data.annotation.width}
    onPreview={(scale) => (preview = scale)}
    onCommit={(scale) =>
      data.onScale(`annotation_${data.annotation.id}`, () => scale, false)}
  />
{/if}
