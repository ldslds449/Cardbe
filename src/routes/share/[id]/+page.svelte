<script lang="ts">
  import { onMount } from "svelte";
  import { ModeWatcher, toggleMode } from "mode-watcher";
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import KanbanIcon from "@lucide/svelte/icons/kanban";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import SearchIcon from "@lucide/svelte/icons/search";
  import MoonIcon from "@lucide/svelte/icons/moon";
  import SunIcon from "@lucide/svelte/icons/sun";
  import WifiIcon from "@lucide/svelte/icons/wifi";

  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import CalendarView from "../../components/calendar/calendar_view.svelte";
  import ViewTaskDialog from "../../components/dialog/view_task_dialog.svelte";
  import ReadonlyBoardView from "../../components/share/readonly_board_view.svelte";
  import { deserialize_column, type Column } from "../../type/column.svelte";
  import type { Task } from "../../type/task.svelte";
  import type { ShareSnapshot } from "../../share";

  type PublicShareResponse = {
    id: string;
    updated_at: string;
    expires_at: string | null;
    snapshot: ShareSnapshot;
  };

  let loading = $state(true);
  let error_message = $state("");
  let response = $state<PublicShareResponse | null>(null);
  let columns = $state<Column[]>([]);
  let search_text = $state("");
  let active_view = $state("board");
  let selected_task = $state<Task | null>(null);
  let task_dialog_open = $state(false);
  let calendar_visible_date = $state(new Date());
  let calendar_view_mode = $state<"month" | "week">("month");
  let refresh_in_progress = $state(false);
  let last_checked_at = $state<Date | null>(null);
  let refresh_share: () => Promise<void> = async () => {};

  let all_tasks = $derived(columns.flatMap((column) => column.tasks));
  let matching_task_count = $derived(
    all_tasks.filter((task) => {
      const search = search_text.trim().toLocaleLowerCase();
      if (!search) return true;
      return [task.title, task.description, ...task.labels, ...task.items.map((item) => item.text)]
        .some((value) => value.toLocaleLowerCase().includes(search));
    }).length,
  );

  function open_task(task: Task) {
    selected_task = task;
    task_dialog_open = true;
  }

  function apply_response(body: PublicShareResponse) {
    if (body.updated_at === response?.updated_at) return;
    response = body;
    columns = body.snapshot.columns.map((column) =>
      deserialize_column({ ...column, sort_order: "custom" }),
    );
    if (selected_task) {
      selected_task = columns.flatMap((column) => column.tasks)
        .find((task) => task.id === selected_task?.id) ?? null;
      if (!selected_task) task_dialog_open = false;
    }
    document.title = `${body.snapshot.title} · Cardbe`;
  }

  onMount(() => {
    const controller = new AbortController();
    const open_card = (event: Event) => {
      const task_id = (event as CustomEvent<{ taskId?: string }>).detail?.taskId;
      const task = all_tasks.find((candidate) => candidate.id === task_id);
      if (task) open_task(task);
    };
    window.addEventListener("cardbe:open-card", open_card);

    const id = window.location.pathname.split("/").filter(Boolean).at(-1) ?? "";
    refresh_share = async () => {
      if (refresh_in_progress) return;
      refresh_in_progress = true;
      try {
        const result = await fetch(`/api/shares/${encodeURIComponent(id)}`, {
          signal: controller.signal,
          cache: "no-store",
        });
        const body = await result.json();
        if (!result.ok) throw new Error(body.error || "Share unavailable");
        apply_response(body as PublicShareResponse);
        error_message = "";
        last_checked_at = new Date();
      } catch (error) {
        if (error instanceof DOMException && error.name === "AbortError") return;
        response = null;
        columns = [];
        selected_task = null;
        task_dialog_open = false;
        document.title = "Share unavailable - Cardbe";
        error_message = error instanceof Error ? error.message : "Share unavailable";
      } finally {
        loading = false;
        refresh_in_progress = false;
      }
    };

    void refresh_share();
    const refresh_interval = window.setInterval(() => void refresh_share(), 180_000);

    return () => {
      window.clearInterval(refresh_interval);
      controller.abort();
      window.removeEventListener("cardbe:open-card", open_card);
    };
  });
</script>

<svelte:head>
  <meta name="robots" content="noindex,nofollow" />
</svelte:head>

<ModeWatcher />

{#snippet theme_toggle()}
  <Button
    onclick={toggleMode}
    variant="outline"
    size="icon"
    class="relative shrink-0"
    aria-label="Toggle color theme"
    title="Toggle color theme"
  >
    <SunIcon
      class="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 !transition-all dark:-rotate-90 dark:scale-0"
    />
    <MoonIcon
      class="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 !transition-all dark:rotate-0 dark:scale-100"
    />
  </Button>
{/snippet}

{#if loading}
  <main class="relative grid min-h-screen place-items-center bg-muted/30 p-6">
    <div class="absolute right-4 top-4">{@render theme_toggle()}</div>
    <div class="flex items-center gap-3 text-sm text-muted-foreground" role="status">
      <span class="size-4 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground"></span>
      Loading shared board…
    </div>
  </main>
{:else if error_message || !response}
  <main class="relative grid min-h-screen place-items-center bg-muted/30 p-6">
    <div class="absolute right-4 top-4">{@render theme_toggle()}</div>
    <section class="w-full max-w-md rounded-xl border bg-card p-8 text-center shadow-sm">
      <h1 class="text-xl font-semibold">Share unavailable</h1>
      <p class="mt-2 text-sm text-muted-foreground">{error_message || "This share no longer exists."}</p>
      <Button
        class="mt-5"
        variant="outline"
        disabled={refresh_in_progress}
        onclick={() => void refresh_share()}
      >
        <RefreshCwIcon class={refresh_in_progress ? "size-4 animate-spin" : "size-4"} />
        Try again
      </Button>
    </section>
  </main>
{:else}
  <main class="flex h-screen min-h-0 flex-col overflow-hidden bg-background">
    <header class="z-30 shrink-0 border-b bg-background/95 px-5 py-3 shadow-xs backdrop-blur">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="min-w-0">
          <div class="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <WifiIcon class="size-3.5" />
            Live read-only LAN share
          </div>
          <h1 class="truncate text-xl font-semibold tracking-tight">{response.snapshot.title}</h1>
          <p class="text-xs text-muted-foreground">
            Published {new Date(response.updated_at).toLocaleString()}
          </p>
        </div>

        <div class="flex w-full items-center gap-2 sm:w-auto">
          <label class="relative min-w-0 flex-1 sm:w-72">
            <SearchIcon class="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input bind:value={search_text} type="search" class="pl-9" placeholder="Search this board" aria-label="Search shared board" />
          </label>
          <Button
            variant="outline"
            size="icon"
            disabled={refresh_in_progress}
            onclick={() => void refresh_share()}
            aria-label="Refresh shared board"
            title={last_checked_at
              ? `Refresh now (last checked ${last_checked_at.toLocaleTimeString()})`
              : "Refresh now"}
          >
            <RefreshCwIcon class={refresh_in_progress ? "size-4 animate-spin" : "size-4"} />
          </Button>
          {@render theme_toggle()}
        </div>
      </div>

      <div class="mt-3 flex items-center justify-between gap-3">
        <div
          class="relative grid w-56 grid-cols-2 rounded-xl border bg-muted p-1 shadow-inner"
          role="tablist"
          aria-label="Shared board view"
        >
          <Button
            variant={active_view === "board" ? "default" : "ghost"}
            size="sm"
            class={`z-10 w-full gap-1.5 transition-colors ${active_view === "board" ? "shadow-sm" : "text-muted-foreground hover:text-foreground"}`}
            role="tab"
            aria-selected={active_view === "board"}
            aria-current={active_view === "board" ? "page" : undefined}
            onclick={() => active_view = "board"}
          ><KanbanIcon class="size-4" />Board</Button>
          <Button
            variant={active_view === "calendar" ? "default" : "ghost"}
            size="sm"
            class={`z-10 w-full gap-1.5 transition-colors ${active_view === "calendar" ? "shadow-sm" : "text-muted-foreground hover:text-foreground"}`}
            role="tab"
            aria-selected={active_view === "calendar"}
            aria-current={active_view === "calendar" ? "page" : undefined}
            onclick={() => active_view = "calendar"}
          ><CalendarDaysIcon class="size-4" />Calendar</Button>
        </div>
        <span class="text-xs tabular-nums text-muted-foreground">
          Viewing <strong class="font-medium text-foreground">{active_view === "board" ? "Board" : "Calendar"}</strong>
          · {matching_task_count} of {all_tasks.length} tasks
        </span>
      </div>
    </header>

    <section class="min-h-0 flex-1 overflow-auto">
      {#key active_view}
        <div class="h-full animate-in fade-in-0 duration-200">
          {#if active_view === "calendar"}
            <CalendarView
              {columns}
              {search_text}
              read_only
              bind:visible_date={calendar_visible_date}
              bind:view_mode={calendar_view_mode}
              show_archived={false}
              show_recurring_previews={false}
              onViewTask={open_task}
            />
          {:else}
            <ReadonlyBoardView {columns} {search_text} onViewTask={open_task} />
          {/if}
        </div>
      {/key}
    </section>
  </main>

  <ViewTaskDialog bind:open={task_dialog_open} task={selected_task} />
{/if}
