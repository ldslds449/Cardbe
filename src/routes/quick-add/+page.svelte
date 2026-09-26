<script lang="ts">
import { logger } from "$lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { onMount, tick } from "svelte";
import { ModeWatcher } from "mode-watcher";

import CheckIcon from "@lucide/svelte/icons/check";
import ListTodoIcon from "@lucide/svelte/icons/list-todo";
import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
import XIcon from "@lucide/svelte/icons/x";

import { Button } from "$lib/components/ui/button/index.js";
import { Input } from "$lib/components/ui/input/index.js";
import { Textarea } from "$lib/components/ui/textarea/index.js";

interface ColumnSummary {
  id: number;
  name: string;
}
interface BoardsState {
  active_board_id: number;
  boards: Array<{ id: number; name: string }>;
}

type CaptureMode = "task" | "note";

const quick_window = getCurrentWindow();
const requested_mode = new URLSearchParams(window.location.search).get("mode");
const mode: CaptureMode = requested_mode === "note" ? "note" : "task";
const shortcut = mode === "task" ? "Ctrl + Alt + T" : "Ctrl + Alt + N";

let columns = $state<ColumnSummary[]>([]);
let boards = $state<Array<{ id: number; name: string }>>([]);
let active_board_id = $state<number | null>(null);
let selected_board_id = $state("");
let column_id = $state("");
let title = $state("");
let details = $state("");
let database_ready = $state(false);
let loading = $state(mode === "task");
let saving = $state(false);
let error = $state("");
let title_input = $state<HTMLInputElement | null>(null);
let content_root = $state<HTMLElement | null>(null);

const can_save = $derived(
  database_ready &&
    !loading &&
    !saving &&
    (mode === "task"
      ? title.trim().length > 0 && column_id.length > 0
      : title.trim().length > 0 || details.trim().length > 0),
);

const window_width = 480;
let resize_frame: number | undefined;
let resizing = false;
let resize_queued = false;
let mounted = false;
let column_load_generation = 0;

function schedule_window_resize() {
  if (!mounted) return;
  if (resize_frame !== undefined) return;
  resize_frame = requestAnimationFrame(() => {
    resize_frame = undefined;
    void resize_window_to_content();
  });
}

async function resize_window_to_content() {
  if (!mounted || !content_root?.isConnected) return;
  if (resizing) {
    // Keep only one follow-up pass: ResizeObserver can fire while Windows
    // applies the current size, and the latest DOM state is all that matters.
    resize_queued = true;
    return;
  }

  resizing = true;
  let made_resizable = false;
  try {
    // A non-resizable native window can reject setSize on Windows. Temporarily
    // permit the programmatic resize, then immediately restore the intended
    // locked window. The element has no fixed height: its natural box is the
    // single source of truth for both task and note capture.
    await quick_window.setResizable(true);
    made_resizable = true;
    if (!mounted || !content_root?.isConnected) return;
    const content_height = Math.ceil(
      content_root.getBoundingClientRect().height,
    );
    await quick_window.setSize(new LogicalSize(window_width, content_height));
  } catch (caught) {
    logger.warn("quick_add.window_resize.failed", caught);
    console.error("Couldn't resize the quick-add window", caught);
  } finally {
    // Do this in finally: a failed setSize must never leave an always-on-top
    // quick window user-resizable.
    if (made_resizable) {
      try {
        await quick_window.setResizable(false);
      } catch (caught) {
        logger.warn("quick_add.window_resize_lock.failed", caught);
        console.error("Couldn't lock the quick-add window size", caught);
      }
    }
    resizing = false;
  }
  if (resize_queued) {
    resize_queued = false;
    schedule_window_resize();
  }
}

onMount(() => {
  mounted = true;
  const resize_observer = new ResizeObserver(schedule_window_resize);
  if (content_root) resize_observer.observe(content_root);
  schedule_window_resize();
  void document.fonts?.ready?.then(schedule_window_resize);
  void invoke<string | null>("get_startup_error")
    .then((startup_error) => {
      if (startup_error) {
        error = startup_error;
        loading = false;
        return;
      }
      database_ready = true;
      if (mode === "task") void load_columns();
    })
    .catch((caught) => {
      error = `Could not check local data startup status: ${String(caught)}`;
      loading = false;
    });
  const handle_focus = () => {
    // Hidden Tauri windows retain their last size, so measure every time one
    // is shown. Do not listen to `window.resize`: setSize itself emits it and
    // would create an unnecessary resize loop.
    schedule_window_resize();
    if (!database_ready) return;
    error = "";
    if (mode === "task") {
      void load_columns(true);
    } else {
      void tick().then(() => title_input?.focus());
    }
  };
  const handle_keydown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void close_quick_window();
      return;
    }
    if (event.key === "Enter" && event.ctrlKey) {
      event.preventDefault();
      void save();
    }
  };

  window.addEventListener("focus", handle_focus);
  document.addEventListener("keydown", handle_keydown);
  requestAnimationFrame(() => title_input?.focus());
  return () => {
    mounted = false;
    resize_observer.disconnect();
    if (resize_frame !== undefined) cancelAnimationFrame(resize_frame);
    window.removeEventListener("focus", handle_focus);
    document.removeEventListener("keydown", handle_keydown);
  };
});

async function load_columns(reset_to_active = false) {
  const generation = ++column_load_generation;
  loading = true;
  try {
    const state = await invoke<BoardsState>("get_boards");
    if (generation !== column_load_generation) return;
    const next_board_id =
      reset_to_active ||
      !selected_board_id ||
      !state.boards.some((board) => String(board.id) === selected_board_id)
        ? String(state.active_board_id)
        : selected_board_id;
    const next_columns = (
      await invoke<Array<ColumnSummary & { tasks: unknown[] }>>(
        "get_board_columns",
        { boardId: Number(next_board_id) },
      )
    ).map(({ id, name }) => ({ id, name }));
    if (generation !== column_load_generation) return;
    boards = state.boards;
    active_board_id = state.active_board_id;
    selected_board_id = next_board_id;
    columns = next_columns;
    const remembered = window.localStorage.getItem(
      `cardbe.quick-add.column-id.${next_board_id}`,
    );
    column_id = next_columns.some((column) => String(column.id) === remembered)
      ? remembered!
      : next_columns[0]
        ? String(next_columns[0].id)
        : "";
  } catch (caught) {
    logger.error("quick_add.load.failed", caught);
    if (generation !== column_load_generation) return;
    console.error(caught);
    error = "Couldn't load your data. Please try again.";
  } finally {
    if (generation !== column_load_generation) return;
    loading = false;
    await tick();
    schedule_window_resize();
    title_input?.focus();
  }
}

async function select_board() {
  await load_columns();
}

async function close_quick_window() {
  try {
    await quick_window.hide();
  } catch (caught) {
    logger.warn("quick_add.window_hide.failed", caught);
    console.error("Couldn't hide the quick-add window", caught);
  }
}

async function save() {
  if (!can_save) return;
  saving = true;
  error = "";
  try {
    if (mode === "task") {
      const selected_column_id = Number(column_id);
      await invoke<number>("add_task_to_board", {
        boardId: Number(selected_board_id),
        columnId: selected_column_id,
        task: {
          id: -1,
          title: title.trim(),
          description: details.trim(),
          color: "",
          start_time: 0,
          due_time: null,
          labels: [],
          items: [],
          recurrence: null,
        },
      });
      window.localStorage.setItem(
        `cardbe.quick-add.column-id.${selected_board_id}`,
        column_id,
      );
    } else {
      await invoke("create_quick_note", {
        title: title.trim(),
        content: details.trim(),
      });
    }

    await emit("cardbe:data-changed", {
      kind: mode,
      boardId: mode === "task" ? Number(selected_board_id) : undefined,
    });
    title = "";
    details = "";
    await close_quick_window();
  } catch (caught) {
    logger.error(mode === "task" ? "quick_add.task_save.failed" : "quick_add.note_save.failed", caught);
    console.error(caught);
    error =
      mode === "task" && String(caught).includes("board")
        ? String(caught)
        : mode === "task"
          ? "Couldn't add the task. Please try again."
          : "Couldn't add the note. Please try again.";
    await tick();
    schedule_window_resize();
    title_input?.focus();
  } finally {
    saving = false;
  }
}
</script>

<svelte:head>
  <title>{mode === "task" ? "New Task" : "New Note"} · Cardbe</title>
</svelte:head>

<main
  bind:this={content_root}
  class="flex select-none flex-col bg-background p-4 pb-6 text-foreground"
>
  <ModeWatcher />
  <header class="mb-3 shrink-0 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div
        class="flex size-9 items-center justify-center rounded-lg bg-muted text-muted-foreground"
      >
        {#if mode === "task"}
          <ListTodoIcon class="size-4" />
        {:else}
          <StickyNoteIcon class="size-4" />
        {/if}
      </div>
      <div>
        <h1 class="text-base font-semibold tracking-tight">
          {mode === "task" ? "New Task" : "New Note"}
        </h1>
        <p class="mt-0.5 text-xs text-muted-foreground">{shortcut}</p>
      </div>
    </div>
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label="Close quick add"
      onclick={close_quick_window}
    >
      <XIcon />
    </Button>
  </header>

  <form
    class="flex flex-none flex-col gap-3"
    onsubmit={(event) => event.preventDefault()}
  >
    <div class={mode === "task" ? "grid gap-1.5" : undefined}>
      {#if mode === "task"}
        <label
          for="quick-add-task-title"
          class="text-xs font-medium text-muted-foreground"
          >Task</label
        >
      {/if}
      <Input
        id={mode === "task" ? "quick-add-task-title" : undefined}
        bind:ref={title_input}
        bind:value={title}
        maxlength={200}
        placeholder={mode === "task" ? "What needs doing?" : "Note title (optional)"}
        aria-label={mode === "task" ? "Task title" : "Note title"}
        autocomplete="off"
        class="focus-visible:shadow-none focus-visible:ring-2 focus-visible:ring-ring/40"
      />
    </div>

    {#if mode === "task"}
      <div class="grid gap-1.5">
        <label
          for="quick-add-board"
          class="text-xs font-medium text-muted-foreground"
          >Add to board</label
        >
        <select
          id="quick-add-board"
          bind:value={selected_board_id}
          disabled={saving || boards.length === 0}
          onchange={() => void select_board()}
          class="border-input bg-background h-10 w-full rounded-md border px-3 text-sm outline-none transition-[border-color,box-shadow] focus-visible:border-ring focus-visible:shadow-none focus-visible:ring-2 focus-visible:ring-ring/40 disabled:opacity-50"
        >
          {#each boards as target (target.id)}
            <option value={String(target.id)}>{target.name}</option>
          {/each}
        </select>
      </div>
      <div class="grid gap-1.5">
        <label
          for="quick-add-column"
          class="text-xs font-medium text-muted-foreground"
          >Add to column</label
        >
        <select
          id="quick-add-column"
          bind:value={column_id}
          disabled={loading || columns.length === 0}
          aria-label="Task column"
          class="border-input bg-background h-10 w-full rounded-md border px-3 text-sm outline-none transition-[border-color,box-shadow] focus-visible:border-ring focus-visible:shadow-none focus-visible:ring-2 focus-visible:ring-ring/40 disabled:opacity-50"
        >
          {#if columns.length === 0}
            <option value="">No columns yet</option>
          {:else}
            {#each columns as column (column.id)}
              <option value={String(column.id)}>{column.name}</option>
            {/each}
          {/if}
        </select>
      </div>
    {/if}

    <Textarea
      bind:value={details}
      class="h-20 min-h-20 flex-none resize-none focus-visible:shadow-none focus-visible:ring-2 focus-visible:ring-ring/40"
      placeholder={mode === "task" ? "Details (optional)" : "Capture your thought…"}
      aria-label={mode === "task" ? "Task details" : "Note content"}
    />

    <div
      class="flex min-h-10 shrink-0 items-end justify-between gap-3 bg-background pt-1"
    >
      <div aria-live="polite" class="text-xs">
        {#if error}
          <span class="text-destructive">{error}</span>
        {:else if mode === "task" && !loading && columns.length === 0}
          <span class="text-muted-foreground"
            >Create a column in Cardbe before adding a task.</span
          >
        {:else}
          <span class="text-muted-foreground"
            >Ctrl+Enter to add · Esc to close</span
          >
        {/if}
      </div>
      <Button
        type="button"
        size="sm"
        disabled={!can_save}
        onclick={() => void save()}
      >
        <CheckIcon />
        {saving ? "Saving…" : "Add"}
      </Button>
    </div>
  </form>
</main>
