<script lang="ts">
  import {
    confirmUnsavedChanges,
    registerUnsavedChanges,
  } from "#lib/unsaved-changes.js";
  import { translateCommandError } from "#lib/command-errors.js";
  import * as m from "#lib/paraglide/messages.js";
  import { logger } from "#lib/logger.js";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { toast } from "svelte-sonner";
  import { onDestroy, onMount, untrack } from "svelte";
  import { create_note_queue } from "./note-queue";
  import { StoredWidth } from "./stored-width.svelte";
  import { mark_recent } from "../home/recent";

  import PlusIcon from "@lucide/svelte/icons/plus";
  import Maximize2Icon from "@lucide/svelte/icons/maximize-2";
  import PanelLeftCloseIcon from "@lucide/svelte/icons/panel-left-close";
  import PanelLeftOpenIcon from "@lucide/svelte/icons/panel-left-open";
  import SearchIcon from "@lucide/svelte/icons/search";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import PinIcon from "@lucide/svelte/icons/pin";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import XIcon from "@lucide/svelte/icons/x";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";

  import { Button } from "#lib/components/ui/button/index.js";
  import * as AlertDialog from "#lib/components/ui/alert-dialog/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import * as InputGroup from "#lib/components/ui/input-group/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Textarea } from "#lib/components/ui/textarea/index.js";
  import EditorControls from "#lib/components/editor/editor-controls.svelte";
  import { editorMode } from "#lib/components/editor/mode.js";
  import CardReferencePicker, {
    type CardReferenceOption,
  } from "#lib/components/editor/card-reference-picker.svelte";
  import {
    RichTextEditor,
    type RichTextEditorHandle,
  } from "#lib/components/editor/index.js";
  import { cn } from "#lib/utils.js";
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
    variant = "panel",
    requested_note = null,
    onNotesChanged,
    onOpenPage,
    open = $bindable(false),
    selected_id = $bindable(null),
    card_reference_options = [],
  }: {
    /** A side panel docked beside the board, or the full Notes page. */
    variant?: "panel" | "page";
    open: boolean;
    selected_id?: number | null;
    card_reference_options?: CardReferenceOption[];
    requested_note?: { id: number } | null;
    onNotesChanged?: () => void;
    onOpenPage?: () => void;
  } = $props();

  const panel_width = new StoredWidth(
    "cardbe.note-panel-width",
    480,
    320,
    960,
    -1,
  );
  // The page has more room, so its list keeps its own width.
  const list_width = untrack(() =>
    variant === "page"
      ? new StoredWidth("cardbe.note-list-width.page", 256, 160, 480, 1)
      : new StoredWidth("cardbe.note-list-width.panel", 208, 160, 360, 1),
  );

  let notes = $state<Note[]>([]);
  let search_text = $state("");
  let loading = $state(false);
  let loaded = $state(false);
  let creating = $state(false);
  let deleting_id = $state<number | null>(null);
  let delete_confirm_open = $state(false);
  let note_to_delete = $state<Note | null>(null);
  const enqueue = create_note_queue();
  let list_collapsed = $state(false);
  let note_ref = $state<RichTextEditorHandle | null>(null);
  let note_language = $state<string | null>(null);
  let note_previewing = $state(false);
  let reference_open = $state(false);
  let reference_start = $state<number | undefined>(undefined);
  const save_timers = new Map<number, ReturnType<typeof setTimeout>>();
  const save_versions = new Map<number, number>();
  const unsaved_notes = new Set<number>();
  let reload_requested = false;
  let edit_generation = 0;

  const selected_note = $derived(notes.find((note) => note.id === selected_id));
  $effect(() => {
    if (selected_note) {
      mark_recent("note", selected_note.id);
    }
  });
  $effect(() => {
    const request = requested_note;
    if (request) {
      selected_id = request.id;
      search_text = "";
      untrack(() => {
        if (!notes.some((note) => note.id === request.id)) {
          loaded = false;
        }
      });
    }
  });
  function handle_note_change(content: string) {
    if (!selected_note) {
      return;
    }
    selected_note.content = content;
    queue_save(selected_note);
    const handle = note_ref;
    // The rich editor opens its own picker at the cursor.
    if ($editorMode === "plain" && handle?.getTextBeforeCursor(2) === "[[") {
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
  // Ranked note IDs from `search_notes`; null until the current query resolves.
  let search_results = $state<number[] | null>(null);
  let search_revision = $state(0);
  let search_generation = 0;
  $effect(() => {
    const query = search_text.trim();
    void search_revision;
    const generation = ++search_generation;
    if (!query) {
      search_results = null;
      return;
    }
    const timer = setTimeout(async () => {
      try {
        const ids = await invoke<number[]>("search_notes", { query });
        if (generation === search_generation) {
          search_results = ids;
        }
      } catch (error) {
        if (generation === search_generation) {
          logger.error("note.search.failed", error);
          toast.error(translateCommandError(error));
        }
      }
    }, 160);
    return () => clearTimeout(timer);
  });
  const filtered_notes = $derived.by(() => {
    const query = search_text.trim().toLocaleLowerCase();
    if (!query) {
      return notes;
    }
    if (search_results === null) {
      return notes.filter((note) =>
        `${note.title}\n${note.content}`.toLocaleLowerCase().includes(query),
      );
    }
    const by_id = new Map(notes.map((note) => [note.id, note]));
    return search_results.flatMap((id) => by_id.get(id) ?? []);
  });

  $effect(() => {
    const is_open = open;
    untrack(() => {
      if (is_open && !loaded && !loading) {
        void load_notes();
      }
      if (!is_open) {
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
    const unregister_drafts = registerUnsavedChanges(
      () => unsaved_notes.size > 0,
    );
    const data_changed_listener = listen<{ kind?: string }>(
      "cardbe:data-changed",
      (event) => {
        if (event.payload.kind !== "note") {
          return;
        }
        if (unsaved_notes.size > 0) {
          reload_requested = true;
          return;
        }
        loaded = false;
        if (open && !loading) {
          void load_notes();
        }
      },
    );
    return () => {
      unregister_drafts();
      void data_changed_listener.then((unlisten) => unlisten());
    };
  });

  async function load_notes() {
    if (unsaved_notes.size > 0) {
      reload_requested = true;
      return;
    }
    reload_requested = false;
    loading = true;
    const generation = edit_generation;
    try {
      const fetched_notes = await invoke<Note[]>("get_notes");
      if (unsaved_notes.size > 0 || generation !== edit_generation) {
        reload_requested = true;
        return;
      }
      notes = fetched_notes;
      search_revision++;
      selected_id = notes.some((note) => note.id === selected_id)
        ? selected_id
        : (notes[0]?.id ?? null);
      loaded = true;
    } catch (error) {
      logger.error("note.load.failed", error);
      toast.error(translateCommandError(error));
    } finally {
      loading = false;
      if (reload_requested && open && unsaved_notes.size === 0) {
        void load_notes();
      }
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
      toast.error(translateCommandError(error));
    } finally {
      creating = false;
    }
  }

  function queue_save(note: Note) {
    edit_generation++;
    unsaved_notes.add(note.id);
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
      onNotesChanged?.();
      const index = notes.findIndex((candidate) => candidate.id === saved.id);
      if (index !== -1 && save_versions.get(saved.id) === version) {
        notes[index].created_at = saved.created_at;
        notes[index].updated_at = saved.updated_at;
        unsaved_notes.delete(saved.id);
        if (search_text.trim()) {
          search_revision++;
        }
        if (unsaved_notes.size === 0 && reload_requested && open && !loading) {
          void load_notes();
        }
      }
    } catch (error) {
      logger.error("note.save.failed", error);
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
      onNotesChanged?.();
      save_versions.delete(note.id);
      unsaved_notes.delete(note.id);
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
      toast.error(translateCommandError(error));
    } finally {
      deleting_id = null;
    }
  }

  function select_note(note_id: number) {
    selected_id = note_id;
  }

  async function set_open(value: boolean) {
    if (value) {
      open = true;
      return;
    }
    flush_saves();
    await enqueue(async () => undefined);
    if (await confirmUnsavedChanges(() => unsaved_notes.size > 0)) {
      if (unsaved_notes.size > 0) {
        unsaved_notes.clear();
        loaded = false;
      }
      open = false;
    }
  }

  function select_note_with_keyboard(event: KeyboardEvent, note_id: number) {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    select_note(note_id);
  }
</script>

{#snippet new_note_button()}
  <Button
    size="sm"
    onclick={create_note}
    disabled={creating || loading || !loaded}
  >
    <PlusIcon />
    {m.note_new()}
  </Button>
{/snippet}

{#if variant === "panel"}
  <aside
    class="relative flex min-h-0 shrink-0 flex-col border-l bg-background"
    style:width="min({panel_width.value}px, 60%, calc(100% - 20rem))"
    aria-labelledby="note-panel-title"
  >
    <button
      type="button"
      class="group absolute inset-y-0 -start-1.5 z-10 flex w-3 cursor-ew-resize items-center justify-center outline-none"
      aria-label={m.common_resize_panel()}
      title={m.common_resize_panel_hint()}
      onpointerdown={(event) => panel_width.start(event)}
      onkeydown={(event) => panel_width.keydown(event)}
      ondblclick={() => panel_width.reset()}
    >
      <span
        class="h-full w-px transition-all group-hover:w-0.5 group-hover:bg-ring group-focus-visible:w-0.5 group-focus-visible:bg-ring"
      ></span>
    </button>
    <header class="flex items-center gap-2 border-b px-3 py-2">
      <h2
        id="note-panel-title"
        class="flex min-w-0 flex-1 items-center gap-2 font-semibold"
      >
        <StickyNoteIcon class="size-4 shrink-0 text-primary" />
        <span class="truncate">{m.note_title()}</span>
      </h2>
      {@render new_note_button()}
      {#if onOpenPage}
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={m.note_open_page()}
          title={m.note_open_page()}
          onclick={onOpenPage}
        >
          <Maximize2Icon />
        </Button>
      {/if}
      <Button
        size="icon-sm"
        variant="ghost"
        aria-label={m.notes_close()}
        title={m.notes_close()}
        onclick={() => void set_open(false)}
      >
        <XIcon />
      </Button>
    </header>
    {@render workspace()}
  </aside>
{:else}
  <section
    class="flex min-h-0 min-w-0 flex-1 flex-col"
    aria-labelledby="note-page-title"
  >
    <header class="flex items-center justify-between gap-3 border-b px-6 py-4">
      <div class="min-w-0">
        <h1
          id="note-page-title"
          class="flex items-center gap-2 text-lg font-semibold tracking-tight"
        >
          <StickyNoteIcon class="size-5 text-primary" />
          {m.note_title()}
        </h1>
        <p class="text-sm text-muted-foreground">{m.note_description()}</p>
      </div>
      {@render new_note_button()}
    </header>
    {@render workspace()}
  </section>
{/if}

{#snippet workspace()}
  <div
    class="grid min-h-0 flex-1"
    style:grid-template-columns={list_collapsed
      ? "minmax(0, 1fr)"
      : `min(${list_width.value}px, 50%) minmax(0, 1fr)`}
  >
    <div
      class="relative flex min-h-0 flex-col border-r"
      hidden={list_collapsed}
    >
      <button
        type="button"
        class="group absolute inset-y-0 -end-1.5 z-10 flex w-3 cursor-ew-resize justify-center outline-none"
        aria-label={m.note_resize_list()}
        title={m.common_resize_panel_hint()}
        onpointerdown={(event) => list_width.start(event)}
        onkeydown={(event) => list_width.keydown(event)}
        ondblclick={() => list_width.reset()}
      >
        <span
          class="h-full w-0.5 transition-colors group-hover:bg-ring group-focus-visible:bg-ring"
        ></span>
      </button>
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
                onkeydown={(event) => select_note_with_keyboard(event, note.id)}
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
    </div>

    <div class="@container/note-header flex min-h-0 min-w-0 flex-col">
      {#if selected_note}
        <div class="flex items-center gap-2 border-b p-3">
          <div class="flex min-w-0 flex-1 items-center gap-2">
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={list_collapsed
                ? m.note_show_list()
                : m.note_hide_list()}
              title={list_collapsed ? m.note_show_list() : m.note_hide_list()}
              aria-pressed={!list_collapsed}
              onclick={() => (list_collapsed = !list_collapsed)}
            >
              {#if list_collapsed}
                <PanelLeftOpenIcon />
              {:else}
                <PanelLeftCloseIcon />
              {/if}
            </Button>
            <Textarea
              rows={1}
              class="min-h-9 min-w-0 flex-1 resize-none break-normal border-0 bg-transparent px-0 py-1 text-lg font-semibold leading-7 shadow-none [overflow-wrap:anywhere] focus-visible:ring-0 md:text-lg dark:bg-transparent"
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
          </div>
          <div class="ml-auto flex shrink-0 items-center gap-2 [&>*]:shrink-0">
            <EditorControls
              compact
              bind:previewing={note_previewing}
              language={note_language}
              disabled={deleting_id === selected_note.id}
              onlanguagechange={(language) =>
                note_ref?.setCodeLanguage?.(language)}
            />
            <div class="flex shrink-0 items-center gap-2">
              <div class="size-8 shrink-0">
                <CardReferencePicker
                  compact
                  disabled={note_previewing || deleting_id === selected_note.id}
                  editor={note_ref}
                  options={card_reference_options}
                  bind:open={reference_open}
                  bind:triggerStart={reference_start}
                />
              </div>
              <div class="hidden items-center gap-2 @min-md/note-header:flex">
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
                  onclick={() => {
                    note_to_delete = selected_note;
                    delete_confirm_open = true;
                  }}
                >
                  <Trash2Icon />
                </Button>
              </div>
              <div class="@min-md/note-header:hidden">
                <DropdownMenu.Root>
                  <DropdownMenu.Trigger>
                    {#snippet child({ props })}
                      <Button
                        {...props}
                        variant="ghost"
                        size="icon-sm"
                        aria-label={m.note_actions()}
                        title={m.note_actions()}
                      >
                        <EllipsisIcon />
                      </Button>
                    {/snippet}
                  </DropdownMenu.Trigger>
                  <DropdownMenu.Content align="end">
                    <DropdownMenu.Item
                      disabled={deleting_id === selected_note.id}
                      onSelect={() => toggle_pinned(selected_note)}
                    >
                      <PinIcon />
                      {selected_note.pinned ? m.note_unpin() : m.note_pin()}
                    </DropdownMenu.Item>
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item
                      variant="destructive"
                      disabled={deleting_id !== null}
                      onSelect={() => {
                        note_to_delete = selected_note;
                        delete_confirm_open = true;
                      }}
                    >
                      <Trash2Icon />
                      {m.note_delete()}
                    </DropdownMenu.Item>
                  </DropdownMenu.Content>
                </DropdownMenu.Root>
              </div>
            </div>
          </div>
        </div>
        <div class="flex min-h-0 flex-1 flex-col bg-background">
          {#key selected_note.id}
            <RichTextEditor
              bind:ref={note_ref}
              bind:language={note_language}
              bind:previewing={note_previewing}
              showControls={false}
              containerClass="flex min-h-0 flex-1 flex-col"
              class="min-h-0 flex-1 rounded-none border-0 bg-transparent px-3 py-2 font-sans text-base leading-6 shadow-none focus-visible:ring-0 md:text-base dark:bg-transparent"
              placeholder={m.note_write()}
              showToolbar={variant === "page"}
              aria-label={m.note_content()}
              cardReferenceOptions={card_reference_options}
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
    </div>
  </div>
{/snippet}

<AlertDialog.Root bind:open={delete_confirm_open}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{m.note_delete()}</AlertDialog.Title>
      <AlertDialog.Description>
        {m.note_delete_confirm({
          name: note_to_delete?.title.trim() || m.note_untitled(),
        })}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{m.common_cancel()}</AlertDialog.Cancel>
      <AlertDialog.Action
        variant="destructive"
        disabled={deleting_id !== null || !note_to_delete}
        onclick={() => {
          if (note_to_delete) {
            void delete_note(note_to_delete);
          }
        }}>{m.common_delete()}</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
