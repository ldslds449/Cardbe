<script lang="ts">
import { logger } from "$lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "svelte-sonner";
import { onDestroy, onMount, untrack } from "svelte";
import { create_note_queue } from "./note-queue";

import PlusIcon from "@lucide/svelte/icons/plus";
import EyeIcon from "@lucide/svelte/icons/eye";
import PencilIcon from "@lucide/svelte/icons/pencil";
import SearchIcon from "@lucide/svelte/icons/search";
import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
import PinIcon from "@lucide/svelte/icons/pin";
import Trash2Icon from "@lucide/svelte/icons/trash-2";
import XIcon from "@lucide/svelte/icons/x";

import { Button } from "$lib/components/ui/button/index.js";
import * as InputGroup from "$lib/components/ui/input-group/index.js";
import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
import * as Sheet from "$lib/components/ui/sheet/index.js";
import { Textarea } from "$lib/components/ui/textarea/index.js";
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

let { open = $bindable(false) }: { open: boolean } = $props();

let notes = $state<Note[]>([]);
let selected_id = $state<number | null>(null);
let search_text = $state("");
let loading = $state(false);
let loaded = $state(false);
let creating = $state(false);
let deleting_id = $state<number | null>(null);
const enqueue = create_note_queue();
let preview_mode = $state(false);
const save_timers = new Map<number, ReturnType<typeof setTimeout>>();
const save_versions = new Map<number, number>();

const selected_note = $derived(notes.find((note) => note.id === selected_id));
const filtered_notes = $derived.by(() => {
  const query = search_text.trim().toLocaleLowerCase();
  return query
    ? notes.filter((note) =>
        `${note.title}\n${note.content}`.toLocaleLowerCase().includes(query),
      )
    : notes;
});

$effect(() => {
  const is_open = open;
  untrack(() => {
    if (is_open && !loaded && !loading) void load_notes();
    if (!is_open) flush_saves();
  });
});

function flush_saves() {
  for (const [id, timer] of save_timers) {
    clearTimeout(timer);
    const note = notes.find((candidate) => candidate.id === id);
    if (note) void save_note(note, save_versions.get(id)!);
  }
  save_timers.clear();
}

onDestroy(flush_saves);

onMount(() => {
  const data_changed_listener = listen<{ kind?: string }>(
    "cardbe:data-changed",
    (event) => {
      if (event.payload.kind !== "note") return;
      loaded = false;
      if (open && !loading) void load_notes();
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
    toast.error("Couldn't load notes");
  } finally {
    loading = false;
  }
}

async function create_note() {
  if (creating || loading || !loaded) return;
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
    preview_mode = false;
  } catch (error) {
    logger.error("note.create.failed", error);
    console.error(error);
    toast.error("Couldn't create note");
  } finally {
    creating = false;
  }
}

function queue_save(note: Note) {
  const existing = save_timers.get(note.id);
  if (existing) clearTimeout(existing);
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
    toast.error("Couldn't save note");
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
  if (deleting_id !== null) return;
  if (!window.confirm(`Delete “${note.title.trim() || "Untitled note"}”?`))
    return;
  const timer = save_timers.get(note.id);
  if (timer) clearTimeout(timer);
  save_timers.delete(note.id);
  deleting_id = note.id;
  try {
    // Persist pending edits first so a failed delete cannot discard them.
    await save_note(note, save_versions.get(note.id) ?? 0);
    await enqueue(() => invoke("delete_note", { id: note.id }));
    save_versions.delete(note.id);
    const index = notes.findIndex((candidate) => candidate.id === note.id);
    if (index !== -1) notes.splice(index, 1);
    if (selected_id === note.id) {
      selected_id = notes[Math.min(index, notes.length - 1)]?.id ?? null;
    }
    toast.success("Note deleted");
  } catch (error) {
    logger.error("note.delete.failed", error);
    console.error(error);
    toast.error("Couldn't delete note");
  } finally {
    deleting_id = null;
  }
}

function select_note(note_id: number) {
  selected_id = note_id;
}

function select_note_with_keyboard(event: KeyboardEvent, note_id: number) {
  if (event.key !== "Enter" && event.key !== " ") return;
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
            Notes
          </Sheet.Title>
          <Sheet.Description
            >Ideas and reminders, saved automatically.</Sheet.Description
          >
        </div>
        <Button
          size="sm"
          onclick={create_note}
          disabled={creating || loading || !loaded}
        >
          <PlusIcon />
          New note
        </Button>
      </div>
    </Sheet.Header>

    <div class="note-layout grid min-h-0 flex-1">
      <aside class="flex min-h-0 flex-col border-r">
        <div class="p-3">
          <InputGroup.Root>
            <InputGroup.Input
              placeholder="Search notes"
              aria-label="Search notes"
              bind:value={search_text}
            />
            <InputGroup.Addon><SearchIcon /></InputGroup.Addon>
            {#if search_text}
              <InputGroup.Addon align="inline-end">
                <InputGroup.Button
                  size="icon-xs"
                  aria-label="Clear search"
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
                Loading notes…
              </p>
            {:else if !loaded}
              <Button variant="outline" onclick={load_notes}
                >Retry loading notes</Button
              >
            {:else}
              {#each filtered_notes as note (note.id)}
                <div
                  role="button"
                  tabindex="0"
                  class={cn(
                    "w-full cursor-pointer rounded-lg border bg-card p-3 text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                    selected_id === note.id && "border-primary bg-accent ring-1 ring-primary/30",
                  )}
                  onclick={() => select_note(note.id)}
                  onkeydown={(event) => select_note_with_keyboard(event, note.id)}
                >
                  <div class="flex items-start gap-2">
                    <span class="min-w-0 flex-1 truncate text-sm font-semibold">
                      {note.title.trim() || "Untitled note"}
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
                    <p class="mt-1 text-xs text-muted-foreground">Empty note</p>
                  {/if}
                </div>
              {:else}
                <p class="py-8 text-center text-sm text-muted-foreground">
                  {search_text.trim() ? "No matching notes" : "No notes yet"}
                </p>
              {/each}
            {/if}
          </div>
        </ScrollArea>
      </aside>

      <main class="note-editor flex min-h-0 min-w-0 flex-col">
        {#if selected_note}
          <div class="flex items-center gap-2 border-b p-3">
            <div class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
              <Button
                size="sm"
                variant={preview_mode ? "ghost" : "secondary"}
                aria-label="Edit note"
                title="Edit note"
                onclick={() => (preview_mode = false)}
              >
                <PencilIcon /> <span class="note-toolbar-label">Edit</span>
              </Button>
              <Button
                size="sm"
                variant={preview_mode ? "secondary" : "ghost"}
                aria-label="Preview note"
                title="Preview note"
                onclick={() => (preview_mode = true)}
              >
                <EyeIcon /> <span class="note-toolbar-label">Preview</span>
              </Button>
            </div>
            <div class="flex shrink-0 items-center gap-2">
              <Button
                size="icon-sm"
                variant={selected_note.pinned ? "secondary" : "ghost"}
                aria-label={selected_note.pinned ? "Unpin note" : "Pin note"}
                title={selected_note.pinned ? "Unpin note" : "Pin note"}
                disabled={deleting_id === selected_note.id}
                onclick={() => toggle_pinned(selected_note)}
              >
                <PinIcon class={cn(selected_note.pinned && "fill-current")} />
              </Button>
              <Button
                size="icon-sm"
                variant="ghost"
                class="text-destructive hover:text-destructive"
                aria-label="Delete note"
                title="Delete note"
                disabled={deleting_id !== null}
                onclick={() => delete_note(selected_note)}
              >
                <Trash2Icon />
              </Button>
            </div>
          </div>
          <div class="flex min-h-0 flex-1 flex-col gap-3 bg-background p-4">
            {#if preview_mode}
              <h1
                class="min-w-0 text-lg font-semibold leading-7 [overflow-wrap:anywhere]"
              >
                {selected_note.title.trim() || "Untitled note"}
              </h1>
            {:else}
              <Textarea
                rows={1}
                class="min-h-9 shrink-0 resize-none border-0 bg-transparent px-0 py-1 text-lg font-semibold leading-7 shadow-none [overflow-wrap:anywhere] focus-visible:ring-0 md:text-lg dark:bg-transparent"
                placeholder="Note title"
                aria-label="Note title"
                disabled={deleting_id === selected_note.id}
                value={selected_note.title}
                onkeydown={(event) => {
                  if (event.key === "Enter") event.preventDefault();
                }}
                oninput={(event) => {
                  const title = event.currentTarget.value.replace(/[\r\n]+/g, " ");
                  event.currentTarget.value = title;
                  selected_note.title = title;
                  queue_save(selected_note);
                }}
              />
            {/if}
            {#if preview_mode}
              <ScrollArea
                class="min-h-0 min-w-0 w-full flex-1"
                orientation="vertical"
              >
                <div class="min-w-0 max-w-full pb-4 pr-3">
                  {#if selected_note.content.trim()}
                    <Markdown md={selected_note.content} />
                  {:else}
                    <p class="text-sm text-muted-foreground">
                      Nothing to preview yet.
                    </p>
                  {/if}
                </div>
              </ScrollArea>
            {:else}
              <Textarea
                class="min-h-0 flex-1 resize-none border-0 bg-transparent px-0 font-sans text-base leading-6 shadow-none focus-visible:ring-0 md:text-base dark:bg-transparent"
                placeholder="Write Markdown…"
                aria-label="Note content"
                disabled={deleting_id === selected_note.id}
                value={selected_note.content}
                oninput={(event) => {
                  selected_note.content = event.currentTarget.value;
                  queue_save(selected_note);
                }}
              />
            {/if}
          </div>
        {:else}
          <div
            class="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground"
          >
            <StickyNoteIcon class="size-10 opacity-50" />
            <p class="text-sm">Select a note or create a new one.</p>
          </div>
        {/if}
      </main>
    </div>
  </Sheet.Content>
</Sheet.Root>

<style>
.note-layout {
  grid-template-columns: clamp(10rem, 36%, 15rem) minmax(0, 1fr);
}

.note-editor {
  container-type: inline-size;
}

.note-toolbar-label {
  display: none;
}

@container (min-width: 20rem) {
  .note-toolbar-label {
    display: inline;
  }
}
</style>
