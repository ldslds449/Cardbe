<script lang="ts">
  import { translateCommandError } from "$lib/command-errors";
  import * as m from "$lib/paraglide/messages.js";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { toast } from "svelte-sonner";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import { create_note_queue } from "./note-queue";

  import PlusIcon from "@lucide/svelte/icons/plus";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import Minimize2Icon from "@lucide/svelte/icons/minimize-2";
  import SearchIcon from "@lucide/svelte/icons/search";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import PinIcon from "@lucide/svelte/icons/pin";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import XIcon from "@lucide/svelte/icons/x";

  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as InputGroup from "$lib/components/ui/input-group/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import * as Sheet from "$lib/components/ui/sheet/index.js";
  import { Textarea } from "$lib/components/ui/textarea/index.js";
  import EditorControls from "$lib/components/editor/editor-controls.svelte";
  import CardReferencePicker, {
    type CardReferenceOption,
  } from "$lib/components/editor/card-reference-picker.svelte";
  import {
    RichTextEditor,
    type RichTextEditorHandle,
  } from "$lib/components/editor/index.js";
  import { cn } from "$lib/utils";
  import Markdown from "../markdown.svelte";

  interface Note {
    id: number;
    title: string;
    content: string;
    pinned: boolean;
    created_at: number;
    updated_at: number;
  }

  let {
    open = $bindable(false),
    card_reference_options = [],
  }: { open: boolean; card_reference_options?: CardReferenceOption[] } =
    $props();

  let notes = $state<Note[]>([]);
  let selected_id = $state<number | null>(null);
  let search_text = $state("");
  let loading = $state(false);
  let loaded = $state(false);
  let creating = $state(false);
  let deleting_id = $state<number | null>(null);
  const enqueue = create_note_queue();
  let note_expanded = $state(false);
  let expanded_note_ref = $state<RichTextEditorHandle | null>(null);
  let note_ref = $state<RichTextEditorHandle | null>(null);
  let note_language = $state<string | null>(null);
  let expanded_note_language = $state<string | null>(null);
  let note_previewing = $state(false);
  let reference_open = $state(false);
  let reference_start = $state<number | undefined>(undefined);
  const save_timers = new Map<number, ReturnType<typeof setTimeout>>();
  const save_versions = new Map<number, number>();

  const selected_note = $derived(notes.find((note) => note.id === selected_id));
  function handle_note_change(content: string) {
    if (!selected_note) {
      return;
    }
    selected_note.content = content;
    queue_save(selected_note);
    const handle = note_expanded ? expanded_note_ref : note_ref;
    if (
      card_reference_options.length &&
      handle?.getTextBeforeCursor(2) === "[["
    ) {
      reference_start = handle.getSelection().from - 2;
      reference_open = true;
    }
  }
  function update_note_title(event: Event) {
    if (!selected_note) {
      return;
    }
    const input = event.currentTarget as HTMLTextAreaElement;
    const title = input.value.replace(/[\r\n]+/g, " ");
    input.value = title;
    selected_note.title = title;
    queue_save(selected_note);
  }
  const filtered_notes = $derived.by(() => {
    const query = search_text.trim().toLocaleLowerCase();
    return query
      ? notes.filter((note) =>
          `${note.title}\n${note.content}`.toLocaleLowerCase().includes(query),
        )
      : notes;
  });

  $effect(() => {
    if (!selected_note) {
      note_expanded = false;
    }
    const is_open = open;
    untrack(() => {
      if (is_open && !loaded && !loading) {
        void load_notes();
      }
      if (!is_open) {
        note_expanded = false;
        flush_saves();
      }
    });
  });

  function flush_saves() {
    for (const [id, timer] of save_timers) {
      clearTimeout(timer);
      const note = notes.find((candidate) => candidate.id === id);
      if (note) {
        void save_note(note, save_versions.get(id)!);
      }
    }
    save_timers.clear();
  }

  onDestroy(flush_saves);

  onMount(() => {
    const data_changed_listener = listen<{ kind?: string }>(
      "cardbe:data-changed",
      (event) => {
        if (event.payload.kind !== "note") {
          return;
        }
        loaded = false;
        if (open && !loading) {
          void load_notes();
        }
      },
    );
    return () => void data_changed_listener.then((unlisten) => unlisten());
  });

  async function load_notes() {
    loading = true;
    try {
      notes = await invoke<Note[]>("get_notes");
      selected_id = notes[0]?.id ?? null;
      loaded = true;
    } catch (error) {
      logger.error("note.load.failed", error);
      console.error(error);
      toast.error(translateCommandError(error));
    } finally {
      loading = false;
    }
  }

  async function create_note() {
    if (creating || loading || !loaded) {
      return;
    }
    creating = true;
    try {
      const note = await invoke<Note>("create_note");
      const first_unpinned = notes.findIndex((candidate) => !candidate.pinned);
      notes.splice(
        first_unpinned === -1 ? notes.length : first_unpinned,
        0,
        note,
      );
      selected_id = note.id;
      search_text = "";
    } catch (error) {
      logger.error("note.create.failed", error);
      console.error(error);
      toast.error(translateCommandError(error));
    } finally {
      creating = false;
    }
  }

  function queue_save(note: Note) {
    const existing = save_timers.get(note.id);
    if (existing) {
      clearTimeout(existing);
    }
    note.updated_at = Date.now();
    const version = (save_versions.get(note.id) ?? 0) + 1;
    save_versions.set(note.id, version);
    save_timers.set(
      note.id,
      setTimeout(() => {
        save_timers.delete(note.id);
        void save_note(note, version);
      }, 400),
    );
  }

  async function save_note(note: Note, version: number) {
    try {
      const snapshot = {
        id: note.id,
        title: note.title,
        content: note.content,
        pinned: note.pinned,
      };
      const saved = await enqueue(() => invoke<Note>("update_note", snapshot));
      const index = notes.findIndex((candidate) => candidate.id === saved.id);
      if (index !== -1 && save_versions.get(saved.id) === version) {
        notes[index].created_at = saved.created_at;
        notes[index].updated_at = saved.updated_at;
      }
    } catch (error) {
      logger.error("note.save.failed", error);
      console.error(error);
      toast.error(translateCommandError(error));
    }
  }

  function toggle_pinned(note: Note) {
    note.pinned = !note.pinned;
    queue_save(note);
    notes.sort(
      (left, right) =>
        Number(right.pinned) - Number(left.pinned) ||
        right.updated_at - left.updated_at,
    );
  }

  async function delete_note(note: Note) {
    if (deleting_id !== null) {
      return;
    }
    if (
      !window.confirm(
        m.note_delete_confirm({ name: note.title.trim() || m.note_untitled() }),
      )
    ) {
      return;
    }
    const timer = save_timers.get(note.id);
    if (timer) {
      clearTimeout(timer);
    }
    save_timers.delete(note.id);
    deleting_id = note.id;
    try {
      // Persist pending edits first so a failed delete cannot discard them.
      await save_note(note, save_versions.get(note.id) ?? 0);
      await enqueue(() => invoke("delete_note", { id: note.id }));
      save_versions.delete(note.id);
      const index = notes.findIndex((candidate) => candidate.id === note.id);
      if (index !== -1) {
        notes.splice(index, 1);
      }
      if (selected_id === note.id) {
        selected_id = notes[Math.min(index, notes.length - 1)]?.id ?? null;
      }
      toast.success(m.ui_note_deleted());
    } catch (error) {
      logger.error("note.delete.failed", error);
      console.error(error);
      toast.error(translateCommandError(error));
    } finally {
      deleting_id = null;
    }
  }

  function select_note(note_id: number) {
    selected_id = note_id;
  }

  function select_note_with_keyboard(event: KeyboardEvent, note_id: number) {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    select_note(note_id);
  }
</script>

<Sheet.Root bind:open>
  <Sheet.Content
    class="gap-0"
    resizable
    defaultWidth={672}
    minWidth={480}
    maxWidth={1200}
    widthStorageKey="cardbe.note-panel-width"
  >
    <Sheet.Header class="border-b pb-4">
      <div class="flex items-center justify-between pr-8">
        <div>
          <Sheet.Title class="flex items-center gap-2">
            <StickyNoteIcon class="size-5 text-primary" />
            {m.note_title()}
          </Sheet.Title>
          <Sheet.Description>{m.note_description()}</Sheet.Description>
        </div>
        <Button
          size="sm"
          onclick={create_note}
          disabled={creating || loading || !loaded}
        >
          <PlusIcon />
          {m.note_new()}
        </Button>
      </div>
    </Sheet.Header>

    <div class="note-layout grid min-h-0 flex-1">
      <aside class="flex min-h-0 flex-col border-r">
        <div class="p-3">
          <InputGroup.Root>
            <InputGroup.Input
              placeholder={m.note_search()}
              aria-label={m.note_search()}
              bind:value={search_text}
            />
            <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
            {#if search_text}
              <InputGroup.Addon align="inline-end">
                <InputGroup.Button
                  size="icon-xs"
                  aria-label={m.board_clear_search()}
                  onclick={() => (search_text = "")}
                >
                  <XIcon />
                </InputGroup.Button>
              </InputGroup.Addon>
            {/if}
          </InputGroup.Root>
        </div>

        <ScrollArea class="min-h-0 flex-1" orientation="vertical">
          <div class="space-y-2 px-3 pb-3">
            {#if loading}
              <p class="py-8 text-center text-sm text-muted-foreground">
                {m.note_loading()}
              </p>
            {:else if !loaded}
              <Button variant="outline" onclick={load_notes}
                >{m.note_retry()}</Button
              >
            {:else}
              {#each filtered_notes as note (note.id)}
                <div
                  role="button"
                  tabindex="0"
                  class={cn(
                    "w-full cursor-pointer rounded-lg border bg-card p-3 text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                    selected_id === note.id &&
                      "border-primary bg-accent ring-1 ring-primary/30",
                  )}
                  onclick={() => select_note(note.id)}
                  onkeydown={(event) =>
                    select_note_with_keyboard(event, note.id)}
                >
                  <div class="flex items-start gap-2">
                    <span
                      class="min-w-0 flex-1 break-normal [overflow-wrap:anywhere] text-sm font-semibold"
                    >
                      {note.title.trim() || m.note_untitled()}
                    </span>
                    {#if note.pinned}
                      <PinIcon class="mt-0.5 size-3.5 shrink-0 fill-current" />
                    {/if}
                  </div>
                  {#if note.content.trim()}
                    <div
                      class="pointer-events-none mt-1 max-h-10 overflow-hidden text-xs text-muted-foreground [&_*]:!m-0 [&_*]:!border-0 [&_*]:!bg-transparent [&_*]:!p-0 [&_*]:!text-xs [&_*]:!leading-5"
                    >
                      <Markdown md={note.content} compact />
                    </div>
                  {:else}
                    <p class="mt-1 text-xs text-muted-foreground">
                      {m.note_empty()}
                    </p>
                  {/if}
                </div>
              {:else}
                <p class="py-8 text-center text-sm text-muted-foreground">
                  {search_text.trim() ? m.note_no_matching() : m.note_none()}
                </p>
              {/each}
            {/if}
          </div>
        </ScrollArea>
      </aside>

      <main class="flex min-h-0 min-w-0 flex-col">
        {#if selected_note}
          <div class="flex flex-wrap items-center gap-2 border-b p-3">
            <Textarea
              rows={1}
              class="min-h-9 min-w-24 flex-1 resize-none break-normal border-0 bg-transparent px-0 py-1 text-lg font-semibold leading-7 shadow-none [overflow-wrap:anywhere] focus-visible:ring-0 md:text-lg dark:bg-transparent"
              placeholder={m.note_name()}
              aria-label={m.note_name()}
              disabled={deleting_id === selected_note.id}
              value={selected_note.title}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                }
              }}
              oninput={update_note_title}
            />
            <EditorControls
              compact
              bind:previewing={note_previewing}
              language={note_language}
              disabled={deleting_id === selected_note.id}
              onlanguagechange={(language) =>
                note_ref?.setCodeLanguage?.(language)}
            />
            <div class="flex shrink-0 items-center gap-2">
              {#if card_reference_options.length}
                <div class="size-8 shrink-0">
                  {#if !note_expanded && !note_previewing}
                    <CardReferencePicker
                      compact
                      disabled={deleting_id === selected_note.id}
                      editor={note_ref}
                      options={card_reference_options}
                      bind:open={reference_open}
                      bind:triggerStart={reference_start}
                    />
                  {/if}
                </div>
              {/if}
              <Button
                size="icon-sm"
                variant="ghost"
                aria-label={m.note_expand_editor()}
                title={m.note_expand_editor()}
                onclick={() => (note_expanded = true)}
              >
                <Maximize2Icon />
              </Button>
              <Button
                size="icon-sm"
                variant={selected_note.pinned ? "secondary" : "ghost"}
                aria-label={selected_note.pinned
                  ? m.note_unpin()
                  : m.note_pin()}
                title={selected_note.pinned ? m.note_unpin() : m.note_pin()}
                disabled={deleting_id === selected_note.id}
                onclick={() => toggle_pinned(selected_note)}
              >
                <PinIcon class={cn(selected_note.pinned && "fill-current")} />
              </Button>
              <Button
                size="icon-sm"
                variant="ghost"
                class="text-destructive hover:text-destructive"
                aria-label={m.note_delete()}
                title={m.note_delete()}
                disabled={deleting_id !== null}
                onclick={() => delete_note(selected_note)}
              >
                <Trash2Icon />
              </Button>
            </div>
          </div>
          <div class="flex min-h-0 flex-1 flex-col gap-3 bg-background p-4">
            {#key selected_note.id}
              <RichTextEditor
                bind:ref={note_ref}
                bind:language={note_language}
                bind:previewing={note_previewing}
                showControls={false}
                containerClass="flex min-h-0 flex-1 flex-col"
                class="min-h-0 flex-1 overflow-y-auto border-0 bg-transparent px-0 font-sans text-base leading-6 shadow-none focus-visible:ring-0 md:text-base dark:bg-transparent"
                placeholder={m.note_write()}
                aria-label={m.note_content()}
                disabled={deleting_id === selected_note.id}
                value={selected_note.content}
                onvaluechange={handle_note_change}
              />
            {/key}
          </div>
        {:else}
          <div
            class="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground"
          >
            <StickyNoteIcon class="size-10 opacity-50" />
            <p class="text-sm">{m.note_choose()}</p>
          </div>
        {/if}
      </main>
    </div>
  </Sheet.Content>
</Sheet.Root>

{#if selected_note}
  <Dialog.Root bind:open={note_expanded}>
    <Dialog.Content
      class="flex h-[min(90vh,56rem)] w-[calc(100vw-1rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl"
      showCloseButton={false}
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        void tick().then(() => expanded_note_ref?.focus());
      }}
    >
      <Dialog.Header class="shrink-0 border-b px-4 py-3 text-start sm:px-6">
        <div class="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3">
          <Dialog.Title class="sr-only"
            >{selected_note.title.trim() || "Untitled note"}</Dialog.Title
          >
          <Textarea
            rows={1}
            class="min-h-9 min-w-0 resize-none break-normal border-0 bg-transparent px-0 py-1 text-lg font-semibold leading-7 shadow-none [overflow-wrap:anywhere] focus-visible:ring-0 md:text-lg dark:bg-transparent"
            placeholder={m.note_name()}
            aria-label={m.note_name()}
            disabled={deleting_id === selected_note.id}
            value={selected_note.title}
            onkeydown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
              }
            }}
            oninput={update_note_title}
          />
          <div class="flex items-center gap-2">
            {#if card_reference_options.length}
              <div class="size-8 shrink-0">
                {#if !note_previewing}
                  <CardReferencePicker
                    compact
                    disabled={deleting_id === selected_note.id}
                    editor={expanded_note_ref}
                    options={card_reference_options}
                    bind:open={reference_open}
                    bind:triggerStart={reference_start}
                  />
                {/if}
              </div>
            {/if}
            <EditorControls
              compact
              bind:previewing={note_previewing}
              language={expanded_note_language}
              disabled={deleting_id === selected_note.id}
              onlanguagechange={(language) =>
                expanded_note_ref?.setCodeLanguage?.(language)}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="Collapse note editor"
              title="Collapse note editor"
              onclick={() => (note_expanded = false)}
            >
              <Minimize2Icon />
            </Button>
          </div>
        </div>
        <Dialog.Description class="sr-only"
          >{m.editor_expanded_description()}</Dialog.Description
        >
      </Dialog.Header>
      <div class="min-h-0 flex-1 p-3 sm:p-5">
        {#key selected_note.id}
          <RichTextEditor
            bind:ref={expanded_note_ref}
            bind:language={expanded_note_language}
            bind:previewing={note_previewing}
            showControls={false}
            containerClass="flex h-full min-h-0 flex-col overflow-hidden rounded-md border bg-background"
            class="min-h-0 flex-1 overflow-y-auto px-3 py-4 sm:px-6"
            placeholder={m.note_write()}
            showToolbar
            aria-label={m.note_content()}
            disabled={deleting_id === selected_note.id}
            value={selected_note.content}
            onvaluechange={handle_note_change}
          />
        {/key}
      </div>
    </Dialog.Content>
  </Dialog.Root>
{/if}

<style>
  .note-layout {
    grid-template-columns: clamp(10rem, 36%, 15rem) minmax(0, 1fr);
  }
</style>
