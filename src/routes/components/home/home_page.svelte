<script lang="ts" module>
  interface ColumnOverview {
    board_id: number;
    id: number;
    name: string;
    done: boolean;
    task_count: number;
  }

  /** One bar of a preview: a board column or a Focus group. */
  interface PreviewRow {
    key: string;
    label: string;
    count: number;
    tone: keyof typeof tone_class;
  }

  interface NoteOverview {
    id: number;
    title: string;
    content: string;
    pinned: boolean;
    updated_at: number;
  }

  interface FocusOverview {
    count: number;
    more: boolean;
    /** Groups of a grouped Focus. */
    rows: PreviewRow[];
    /** The first tasks of an ungrouped Focus. */
    titles: string[];
  }

  // Bars are colored only where the color says something: done columns
  // and low priorities fade, overdue and due-today groups match their due
  // badges, and priorities follow their badges from red to the text color.
  const tone_class = {
    normal: "bg-primary/60",
    muted: "bg-muted-foreground/30",
    overdue: "bg-destructive/70",
    today: "bg-warning/70",
    urgent: "bg-destructive",
    high: "bg-destructive/50",
    medium: "bg-foreground/50",
  };

  // The last overviews, so returning home shows them at once and then
  // refreshes instead of growing the tiles as they load.
  let cached_columns: Map<number, ColumnOverview[]> | null = null;
  let cached_focuses = new Map<string, FocusOverview>();
  let cached_notes: NoteOverview[] | null = null;
</script>

<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { formatDate, formatNumber } from "$lib/i18n";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { untrack } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { by_recent, read_recent } from "./recent";

  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import ListFilterIcon from "@lucide/svelte/icons/list-filter";
  import PinIcon from "@lucide/svelte/icons/pin";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import UsersIcon from "@lucide/svelte/icons/users";
  import type { Component } from "svelte";

  import type { BoardRole } from "../../board.svelte";
  import { MAX_FOCUSES, type Focus } from "../../type/focus";
  import { group_focus_items } from "../focus/focus-groups";
  import { load_focus_results } from "../focus/focus-results";

  // Rows drawn per board or Focus; the rest are summarized.
  const PREVIEW_ROWS = 5;
  // Tasks read per Focus; more are shown as a lower bound.
  const COUNT_LIMIT = 100;
  // Tiles per section, two rows on wide windows; the rest are behind
  // "Show all" (in place) or "View all" (the Notes page).
  const SECTION_TILES = 6;

  let {
    focuses,
    boards,
    onOpenFocus,
    onCreateFocus,
    onOpenBoard,
    onCreateBoard,
    onOpenNotes,
    onOpenNote,
    onCreateNote,
  }: {
    focuses: Focus[];
    boards: {
      id: number;
      name: string;
      task_count: number;
      shared_role: BoardRole;
    }[];
    onOpenFocus: (id: string) => void;
    onCreateFocus: () => void;
    onOpenBoard: (board_id: number) => void;
    onCreateBoard: () => void;
    onOpenNotes: () => void;
    onOpenNote: (note_id: number) => void;
    onCreateNote: () => void;
  } = $props();

  let now = $state(new Date());
  // Null until the first load.
  let columns = $state<Map<number, ColumnOverview[]> | null>(cached_columns);
  let focus_overviews = $state(cached_focuses);
  let notes = $state<NoteOverview[] | null>(cached_notes);
  let recent = $state(read_recent());
  let show_all = $state({ focus: false, board: false });

  // Recently viewed first. Pinned notes stay on top, and a note counts as
  // recent when it was last viewed or edited.
  const sorted_focuses = $derived(
    by_recent(focuses, (view) => recent.focus[view.id]),
  );
  const sorted_boards = $derived(
    by_recent(boards, (item) => recent.board[item.id]),
  );
  const sorted_notes = $derived(
    notes &&
      by_recent(notes, (note) =>
        Math.max(note.updated_at, recent.note[note.id] ?? 0),
      ).sort((left, right) => Number(right.pinned) - Number(left.pinned)),
  );
  let request = 0;

  let greeting = $derived(
    now.getHours() < 12
      ? m.home_greeting_morning()
      : now.getHours() < 18
        ? m.home_greeting_afternoon()
        : m.home_greeting_evening(),
  );
  // Reload when tasks are added, moved, or removed, or a Focus changes.
  let reload_key = $derived(
    JSON.stringify([
      boards.map((board) => [board.id, board.task_count]),
      focuses,
    ]),
  );

  async function load_columns() {
    const rows = await invoke<ColumnOverview[]>("get_board_overviews");
    const grouped = new Map<number, ColumnOverview[]>();
    for (const row of rows) {
      grouped.set(row.board_id, [...(grouped.get(row.board_id) ?? []), row]);
    }
    return grouped;
  }

  async function load_focus_overview(
    view: Focus,
    at: Date,
  ): Promise<[string, FocusOverview]> {
    const page = await load_focus_results(view, at, null, COUNT_LIMIT);
    const grouped = view.group !== "none";
    return [
      view.id,
      {
        count: page.items.length,
        more: page.next_cursor !== null,
        rows: grouped
          ? group_focus_items(page.items, view.group, at).map((group) => ({
              key: group.key,
              label: group.label ?? view.name,
              count: group.items.length,
              tone: group_tone(view.group, group.key),
            }))
          : [],
        titles: grouped
          ? []
          : page.items
              .slice(0, 3)
              .map((item) => item.task.title || m.task_untitled()),
      },
    ];
  }

  function group_tone(group: Focus["group"], key: string): PreviewRow["tone"] {
    if (group === "due" && (key === "overdue" || key === "today")) {
      return key;
    }
    if (group === "priority") {
      return key === "urgent" || key === "high" || key === "medium"
        ? key
        : "muted";
    }
    return "normal";
  }

  function column_rows(board_columns: ColumnOverview[]): PreviewRow[] {
    return board_columns.map((column) => ({
      key: String(column.id),
      label: column.name,
      count: column.task_count,
      tone: column.done ? "muted" : "normal",
    }));
  }

  async function refresh() {
    const current = ++request;
    now = new Date();
    recent = read_recent();
    const at = now;
    const [loaded_columns, loaded_focuses, loaded_notes] =
      await Promise.allSettled([
        load_columns(),
        Promise.all(focuses.map((view) => load_focus_overview(view, at))),
        invoke<NoteOverview[]>("get_notes"),
      ]);
    if (current !== request) {
      return;
    }
    if (loaded_columns.status === "fulfilled") {
      columns = cached_columns = loaded_columns.value;
    } else {
      logger.error("home.columns.failed", loaded_columns.reason);
    }
    if (loaded_focuses.status === "fulfilled") {
      focus_overviews = cached_focuses = new Map(loaded_focuses.value);
    } else {
      logger.error("home.focuses.failed", loaded_focuses.reason);
    }
    if (loaded_notes.status === "fulfilled") {
      notes = cached_notes = loaded_notes.value;
    } else {
      logger.error("home.notes.failed", loaded_notes.reason);
    }
  }

  $effect(() => {
    void reload_key;
    untrack(() => void refresh());
  });

  $effect(() => {
    const unlisten = listen("cardbe:data-changed", () => void refresh());
    return () => {
      void unlisten.then((stop) => stop());
    };
  });

  function focus_detail(overview: FocusOverview | undefined) {
    if (!overview) {
      return undefined;
    }
    return overview.more
      ? m.home_cards_more({ count: overview.count })
      : m.home_board_cards({ count: overview.count });
  }

  /** The first line of a note's Markdown, without its leading markers. */
  function note_excerpt(content: string) {
    const line = content
      .split("\n")
      .map((text) => text.replace(/^\s*(?:[#>*+-]+|\d+\.|- \[[ xX]\])\s*/, ""))
      .find((text) => text.trim());
    return line?.trim() ?? "";
  }

  const tile =
    "flex w-full min-w-0 items-center gap-3 rounded-lg border bg-card p-3 text-left shadow-xs transition-colors outline-none hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50";
</script>

<svelte:window onfocus={() => void refresh()} />

{#snippet tile_header(Icon: Component, title: string, detail?: string)}
  <span class="rounded-md bg-primary/10 p-2 text-primary">
    <Icon class="size-4" aria-hidden="true" />
  </span>
  <span class="min-w-0 flex-1">
    <span class="block truncate text-sm font-medium">{title}</span>
    {#if detail}
      <span class="block text-xs text-muted-foreground">{detail}</span>
    {/if}
  </span>
{/snippet}

{#snippet preview(rows: PreviewRow[], more_label: (count: number) => string)}
  {@const shown = rows.slice(0, PREVIEW_ROWS)}
  {@const widest = Math.max(1, ...shown.map((row) => row.count))}
  <span class="block w-full space-y-1.5">
    {#each shown as row (row.key)}
      <span class="flex items-center gap-2 text-xs">
        <span class="w-20 shrink-0 truncate text-muted-foreground"
          >{row.label}</span
        >
        <span class="h-2 min-w-0 flex-1 overflow-hidden rounded-full bg-muted">
          <span
            class={["block h-full rounded-full", tone_class[row.tone]]}
            style:width="{(row.count / widest) * 100}%"
          ></span>
        </span>
        <span class="w-8 shrink-0 text-right tabular-nums text-muted-foreground"
          >{formatNumber(row.count)}</span
        >
      </span>
    {/each}
  </span>
  {#if rows.length > PREVIEW_ROWS}
    <span class="text-xs text-muted-foreground"
      >{more_label(rows.length - PREVIEW_ROWS)}</span
    >
  {/if}
{/snippet}

{#snippet section_header(
  id: string,
  title: string,
  count: number | null,
  more: "focus" | "board" | "notes" | null,
  add: {
    label: string;
    onclick: () => void;
    disabled?: boolean;
    hint?: string;
  },
)}
  <div class="mb-3 flex min-h-8 items-center gap-2">
    <h2 {id} class="min-w-0 flex-1 font-semibold tracking-tight">
      {title}
      {#if count !== null}
        <span class="font-normal text-muted-foreground"
          >· {formatNumber(count)}</span
        >
      {/if}
    </h2>
    {#if more === "notes"}
      <Button variant="ghost" size="sm" onclick={onOpenNotes}>
        {m.home_view_all()}
      </Button>
    {:else if more}
      <Button
        variant="ghost"
        size="sm"
        aria-expanded={show_all[more]}
        onclick={() => (show_all[more] = !show_all[more])}
      >
        {show_all[more] ? m.home_show_less() : m.home_show_all()}
      </Button>
    {/if}
    <!-- A disabled button gets no pointer events, so its wrapper keeps the hint. -->
    <span class="shrink-0" title={add.hint ?? add.label}>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={add.label}
        disabled={add.disabled}
        onclick={add.onclick}
      >
        <PlusIcon />
      </Button>
    </span>
  </div>
{/snippet}

{#snippet section_empty(label: string)}
  <p
    class="rounded-lg border border-dashed p-4 text-center text-sm text-muted-foreground"
  >
    {label}
  </p>
{/snippet}

{#snippet preview_empty(label: string)}
  <span
    class="flex flex-1 items-center justify-center py-3 text-center text-xs text-muted-foreground"
    >{label}</span
  >
{/snippet}

<ScrollArea class="min-h-0 min-w-0 flex-1" orientation="vertical">
  <div class="mx-auto w-full max-w-5xl space-y-8 p-5 sm:p-8">
    <header>
      <p class="text-sm font-medium text-primary">
        {formatDate(now, { weekday: "long", month: "long", day: "numeric" })}
      </p>
      <h1 class="mt-1 text-3xl font-semibold tracking-tight">{greeting}</h1>
    </header>

    <section aria-labelledby="home-focuses">
      {@render section_header(
        "home-focuses",
        m.focus_section(),
        focuses.length,
        sorted_focuses.length > SECTION_TILES ? "focus" : null,
        {
          label: m.focus_new(),
          onclick: onCreateFocus,
          disabled: focuses.length >= MAX_FOCUSES,
          hint:
            focuses.length >= MAX_FOCUSES
              ? m.focus_limit_reached({ count: MAX_FOCUSES })
              : undefined,
        },
      )}
      {#if focuses.length === 0}
        {@render section_empty(m.focus_list_empty())}
      {:else}
        <ul class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {#each show_all.focus ? sorted_focuses : sorted_focuses.slice(0, SECTION_TILES) as view (view.id)}
            {@const overview = focus_overviews.get(view.id)}
            <li>
              <button
                type="button"
                class={[tile, "h-full flex-col items-stretch"]}
                title={view.name}
                onclick={() => onOpenFocus(view.id)}
              >
                <span class="flex min-w-0 items-center gap-3">
                  {@render tile_header(
                    ListFilterIcon,
                    view.name,
                    focus_detail(overview),
                  )}
                </span>
                {#if overview && overview.count === 0}
                  {@render preview_empty(m.focus_empty())}
                {:else if overview && overview.rows.length > 0}
                  {@render preview(overview.rows, (count) =>
                    m.home_more_groups({ count }),
                  )}
                {:else if overview}
                  <span class="block w-full space-y-1 text-xs">
                    {#each overview.titles as title, index (index)}
                      <span class="flex min-w-0 items-center gap-2">
                        <span
                          class="size-1.5 shrink-0 rounded-full bg-muted-foreground/50"
                          aria-hidden="true"
                        ></span>
                        <span class="truncate">{title}</span>
                      </span>
                    {/each}
                  </span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section aria-labelledby="home-boards">
      {@render section_header(
        "home-boards",
        m.board_list(),
        boards.length,
        sorted_boards.length > SECTION_TILES ? "board" : null,
        { label: m.board_new(), onclick: onCreateBoard },
      )}
      <ul class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {#each show_all.board ? sorted_boards : sorted_boards.slice(0, SECTION_TILES) as item (item.id)}
          {@const board_columns = columns?.get(item.id) ?? []}
          <li>
            <button
              type="button"
              class={[tile, "h-full flex-col items-stretch"]}
              title={item.name}
              onclick={() => onOpenBoard(item.id)}
            >
              <span class="flex min-w-0 items-center gap-3">
                {@render tile_header(
                  item.shared_role === "owner"
                    ? LayoutDashboardIcon
                    : UsersIcon,
                  item.name,
                  m.home_board_cards({ count: item.task_count }),
                )}
              </span>
              {#if board_columns.length > 0}
                {@render preview(column_rows(board_columns), (count) =>
                  m.home_more_columns({ count }),
                )}
              {:else if columns}
                {@render preview_empty(m.ui_no_columns_yet())}
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    </section>

    <section aria-labelledby="home-notes">
      {@render section_header(
        "home-notes",
        m.note_title(),
        notes?.length ?? null,
        sorted_notes && sorted_notes.length > SECTION_TILES ? "notes" : null,
        { label: m.note_new(), onclick: onCreateNote },
      )}
      {#if sorted_notes?.length === 0}
        {@render section_empty(m.note_none())}
      {:else if sorted_notes}
        <ul class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {#each sorted_notes.slice(0, SECTION_TILES) as note (note.id)}
            {@const title = note.title.trim() || m.note_untitled()}
            {@const excerpt = note_excerpt(note.content)}
            <li>
              <button
                type="button"
                class={[tile, "h-full flex-col items-stretch"]}
                {title}
                onclick={() => onOpenNote(note.id)}
              >
                <span class="flex min-w-0 items-center gap-3">
                  {@render tile_header(
                    StickyNoteIcon,
                    title,
                    formatDate(note.updated_at, {
                      month: "short",
                      day: "numeric",
                    }),
                  )}
                  {#if note.pinned}
                    <PinIcon
                      class="size-3.5 shrink-0 fill-current text-muted-foreground"
                      aria-label={m.note_pinned()}
                    />
                  {/if}
                </span>
                <span
                  class="line-clamp-2 text-xs break-normal [overflow-wrap:anywhere] text-muted-foreground"
                  >{excerpt || m.note_empty()}</span
                >
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>
</ScrollArea>
