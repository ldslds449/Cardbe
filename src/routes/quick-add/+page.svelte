<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { emit } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
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

  type CaptureMode = "task" | "note";

  const quick_window = getCurrentWindow();
  const LAST_COLUMN_KEY = "cardbe.quick-add.column-id";
  const requested_mode = new URLSearchParams(window.location.search).get("mode");
  const mode: CaptureMode = requested_mode === "note" ? "note" : "task";
  const shortcut = mode === "task" ? "Ctrl + Alt + T" : "Ctrl + Alt + N";

  let columns = $state<ColumnSummary[]>([]);
  let column_id = $state("");
  let title = $state("");
  let details = $state("");
  let loading = $state(mode === "task");
  let saving = $state(false);
  let error = $state("");
  let title_input = $state<HTMLInputElement | null>(null);

  const can_save = $derived(
    !loading &&
      !saving &&
      (mode === "task"
        ? title.trim().length > 0 && column_id.length > 0
        : title.trim().length > 0 || details.trim().length > 0),
  );

  onMount(() => {
    if (mode === "task") void load_columns();
    const handle_focus = () => {
      error = "";
      if (mode === "task") {
        void load_columns();
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
      window.removeEventListener("focus", handle_focus);
      document.removeEventListener("keydown", handle_keydown);
    };
  });

  async function load_columns() {
    loading = true;
    try {
      columns = (await invoke<Array<ColumnSummary & { tasks: unknown[] }>>(
        "get_columns",
      )).map(({ id, name }) => ({ id, name }));
      const remembered = window.localStorage.getItem(LAST_COLUMN_KEY);
      column_id = columns.some((column) => String(column.id) === remembered)
        ? remembered!
        : columns[0]
          ? String(columns[0].id)
          : "";
    } catch (caught) {
      console.error(caught);
      error = "Couldn't load your data. Please try again.";
    } finally {
      loading = false;
      await tick();
      title_input?.focus();
    }
  }

  async function close_quick_window() {
    try {
      await quick_window.hide();
    } catch (caught) {
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
        await invoke<number>("add_task", {
          columnId: selected_column_id,
          afterTaskId: null,
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
        window.localStorage.setItem(LAST_COLUMN_KEY, column_id);
      } else {
        await invoke("create_quick_note", {
          title: title.trim(),
          content: details.trim(),
        });
      }

      await emit("cardbe:data-changed", { kind: mode });
      title = "";
      details = "";
      await close_quick_window();
    } catch (caught) {
      console.error(caught);
      error = mode === "task" ? "Couldn't add the task. Please try again." : "Couldn't add the note. Please try again.";
      await tick();
      title_input?.focus();
    } finally {
      saving = false;
    }
  }
</script>

<svelte:head>
  <title>{mode === "task" ? "New Task" : "New Note"} · Cardbe</title>
</svelte:head>

<main class="flex h-screen select-none flex-col overflow-hidden bg-background p-5 text-foreground">
  <ModeWatcher />
  <header class="mb-4 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="flex size-9 items-center justify-center rounded-lg bg-muted text-muted-foreground">
        {#if mode === "task"}
          <ListTodoIcon class="size-4" />
        {:else}
          <StickyNoteIcon class="size-4" />
        {/if}
      </div>
      <div>
        <h1 class="text-base font-semibold tracking-tight">{mode === "task" ? "New Task" : "New Note"}</h1>
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

  <form class="flex min-h-0 flex-1 flex-col gap-3" onsubmit={(event) => event.preventDefault()}>
    <Input
      bind:ref={title_input}
      bind:value={title}
      maxlength={200}
      placeholder={mode === "task" ? "What needs doing?" : "Note title (optional)"}
      aria-label={mode === "task" ? "Task title" : "Note title"}
      autocomplete="off"
    />

    {#if mode === "task"}
      <select
        bind:value={column_id}
        disabled={loading || columns.length === 0}
        aria-label="Task column"
        class="border-input bg-background focus-visible:border-ring focus-visible:ring-ring/50 h-9 w-full rounded-md border px-3 text-sm outline-none focus-visible:ring-[3px] disabled:opacity-50"
      >
        {#if columns.length === 0}
          <option value="">No columns yet</option>
        {:else}
          {#each columns as column (column.id)}
            <option value={String(column.id)}>{column.name}</option>
          {/each}
        {/if}
      </select>
    {/if}

    <Textarea
      bind:value={details}
      class="min-h-20 flex-1 resize-none"
      placeholder={mode === "task" ? "Details (optional)" : "Capture your thought…"}
      aria-label={mode === "task" ? "Task details" : "Note content"}
    />

    <div class="flex min-h-8 items-end justify-between gap-3">
      <div aria-live="polite" class="text-xs">
        {#if error}
          <span class="text-destructive">{error}</span>
        {:else if mode === "task" && !loading && columns.length === 0}
          <span class="text-muted-foreground">Create a column in Cardbe before adding a task.</span>
        {:else}
          <span class="text-muted-foreground">Ctrl+Enter to add · Esc to close</span>
        {/if}
      </div>
      <Button type="button" size="sm" disabled={!can_save} onclick={() => void save()}>
        <CheckIcon />
        {saving ? "Saving…" : "Add"}
      </Button>
    </div>
  </form>
</main>
