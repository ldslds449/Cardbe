<script lang="ts">
  import { copyBoardLink } from "#lib/deep-link.js";
  import LinkIcon from "@lucide/svelte/icons/link";
  import { confirmUnsavedChanges } from "#lib/unsaved-changes.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import HouseIcon from "@lucide/svelte/icons/house";
  import PanelLeftCloseIcon from "@lucide/svelte/icons/panel-left-close";
  import PanelLeftOpenIcon from "@lucide/svelte/icons/panel-left-open";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import PictureInPicture2Icon from "@lucide/svelte/icons/picture-in-picture-2";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import SearchIcon from "@lucide/svelte/icons/search";
  import StickyNoteIcon from "@lucide/svelte/icons/sticky-note";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import UsersIcon from "@lucide/svelte/icons/users";
  import ListFilterIcon from "@lucide/svelte/icons/list-filter";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import * as AlertDialog from "#lib/components/ui/alert-dialog/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import * as Kbd from "#lib/components/ui/kbd/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Separator } from "#lib/components/ui/separator/index.js";
  import * as Sheet from "#lib/components/ui/sheet/index.js";
  import type { Snippet } from "svelte";
  import type { BoardRole } from "../../board.svelte";
  import { HOME_PAGE_ID, NOTES_PAGE_ID } from "./workspace";
  import { MAX_FOCUSES, type Focus } from "../../type/focus";
  import { StoredWidth } from "../note/stored-width.svelte";

  export interface BoardItem {
    id: number;
    name: string;
    task_count: number;
    shared_role: BoardRole;
    sync_status: string;
    sync_revision: number;
  }
  let {
    boards,
    active_board_id,
    expanded,
    compact,
    sheet_open = $bindable(),
    onQuickSwitch,
    onToggle,
    onSwitch,
    onCreate,
    onRename,
    onLoadArchivedCount,
    onDelete,
    focuses,
    active_focus_id,
    onOpenFocus,
    onOpenHome,
    onOpenNotes,
    onCreateFocus,
    onEditFocus,
    onFloatFocus,
    onDeleteFocus,
  }: {
    boards: BoardItem[];
    active_board_id: number | null;
    /** Wide windows keep the sidebar beside the content, or as an icon rail. */
    expanded: boolean;
    /** Narrow windows show the sidebar only as a sheet. */
    compact: boolean;
    sheet_open: boolean;
    onQuickSwitch: () => void;
    /** Collapses or expands the sidebar on wide windows. */
    onToggle: () => void;
    onSwitch: (id: number) => void;
    onCreate: (name: string) => Promise<boolean>;
    onRename: (id: number, name: string) => Promise<boolean>;
    onLoadArchivedCount: (id: number) => Promise<number | null>;
    onDelete: (id: number) => Promise<boolean>;
    focuses: Focus[];
    /** The Focus shown instead of the active board, if any. */
    active_focus_id: string | null;
    onOpenFocus: (id: string) => void;
    onOpenHome: () => void;
    onOpenNotes: () => void;
    onCreateFocus: () => void;
    onEditFocus: (id: string) => void;
    onFloatFocus: (id: string) => void;
    onDeleteFocus: (id: string) => Promise<boolean>;
  } = $props();

  const sidebar_width = new StoredWidth(
    "cardbe.sidebar-width",
    240,
    200,
    400,
    1,
  );
  let create_open = $state(false);
  let rename_open = $state(false);
  let delete_open = $state(false);
  let new_name = $state("");
  let rename_name = $state("");
  let selected_board = $state<BoardItem | null>(null);
  let archived_count = $state<number | null>(null);
  let saving = $state(false);
  let delete_view_open = $state(false);
  let selected_view = $state<Focus | null>(null);
  function open_delete_view_dialog(view: Focus) {
    selected_view = view;
    delete_view_open = true;
  }
  async function remove_view() {
    if (!selected_view || saving) {
      return;
    }
    saving = true;
    try {
      if (await onDeleteFocus(selected_view.id)) {
        delete_view_open = false;
      }
    } finally {
      saving = false;
    }
  }
  function select_view(id: string) {
    onOpenFocus(id);
    sheet_open = false;
  }
  function open_home() {
    onOpenHome();
    sheet_open = false;
  }
  function open_notes() {
    onOpenNotes();
    sheet_open = false;
  }
  function quick_switch() {
    sheet_open = false;
    onQuickSwitch();
  }
  export function open_create_dialog() {
    new_name = "";
    create_open = true;
  }
  function open_rename_dialog(item: BoardItem) {
    selected_board = item;
    rename_name = item.name;
    rename_open = true;
  }
  function open_delete_dialog(item: BoardItem) {
    selected_board = item;
    archived_count = null;
    delete_open = true;
    void onLoadArchivedCount(item.id).then((count) => {
      if (selected_board === item) {
        archived_count = count;
      }
    });
  }
  function select_board(id: number) {
    onSwitch(id);
    sheet_open = false;
  }
  async function create() {
    if (!new_name.trim() || saving) {
      return;
    }
    saving = true;
    try {
      if (await onCreate(new_name.trim())) {
        create_open = false;
        sheet_open = false;
      }
    } finally {
      saving = false;
    }
  }
  async function rename() {
    if (!selected_board || !rename_name.trim() || saving) {
      return;
    }
    saving = true;
    try {
      if (await onRename(selected_board.id, rename_name.trim())) {
        rename_open = false;
      }
    } finally {
      saving = false;
    }
  }
  async function remove() {
    if (!selected_board || saving) {
      return;
    }
    saving = true;
    try {
      if (await onDelete(selected_board.id)) {
        delete_open = false;
      }
    } finally {
      saving = false;
    }
  }
</script>

<!-- The ⋯ button offers the right-click menu to pointer and keyboard users. -->
{#snippet actions_button(
  label: string,
  items: Snippet<["context" | "dropdown"]>,
)}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          variant="ghost"
          size="icon-xs"
          class="absolute end-1 top-1/2 -translate-y-1/2 text-muted-foreground opacity-0 group-hover/row:opacity-100 group-focus-within/row:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
          aria-label={label}
          title={label}
        >
          <EllipsisIcon />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content align="start" class="min-w-40">
      {@render items("dropdown")}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
{/snippet}

{#snippet add_button(label: string, disabled: boolean, onclick: () => void)}
  <Button
    variant="ghost"
    size="icon-xs"
    class="text-muted-foreground"
    aria-label={label}
    title={label}
    {disabled}
    {onclick}
  >
    <PlusIcon />
  </Button>
{/snippet}

{#snippet focus_row(view: Focus)}
  {#snippet items(kind: "context" | "dropdown")}
    {@const Item = kind === "context" ? ContextMenu.Item : DropdownMenu.Item}
    {@const Separator =
      kind === "context" ? ContextMenu.Separator : DropdownMenu.Separator}
    <Item
      onclick={() => {
        onEditFocus(view.id);
        sheet_open = false;
      }}
      ><PencilIcon />
      {m.common_edit()}</Item
    >
    <Item onclick={() => onFloatFocus(view.id)}
      ><PictureInPicture2Icon />
      {m.focus_float_on_desktop()}</Item
    >
    <Separator />
    <Item variant="destructive" onclick={() => open_delete_view_dialog(view)}
      ><Trash2Icon />
      {m.common_delete()}</Item
    >
  {/snippet}
  <ContextMenu.Root>
    <ContextMenu.Trigger class="group/row relative block">
      <button
        type="button"
        class={`relative flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md py-1.5 ps-2 pe-8 text-left text-sm transition-colors ${view.id === active_focus_id ? "bg-background font-medium text-foreground shadow-xs before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary" : "text-muted-foreground hover:bg-background/70 hover:text-foreground"}`}
        onclick={() => select_view(view.id)}
        aria-current={view.id === active_focus_id ? "page" : undefined}
        title={view.name}
      >
        <ListFilterIcon
          class="size-4 shrink-0 text-primary"
          aria-hidden="true"
        />
        <span class="min-w-0 flex-1 truncate">{view.name}</span>
      </button>
      {@render actions_button(m.focus_actions({ name: view.name }), items)}
    </ContextMenu.Trigger>
    <ContextMenu.Content>
      {@render items("context")}
    </ContextMenu.Content>
  </ContextMenu.Root>
{/snippet}

{#snippet board_row(item: BoardItem)}
  {#snippet items(kind: "context" | "dropdown")}
    {@const Item = kind === "context" ? ContextMenu.Item : DropdownMenu.Item}
    {@const Separator =
      kind === "context" ? ContextMenu.Separator : DropdownMenu.Separator}
    <Item onclick={() => void copyBoardLink(item.id)}
      ><LinkIcon />{m.deep_link_copy_board()}</Item
    >
    {#if item.shared_role === "owner"}
      <Item onclick={() => open_rename_dialog(item)}
        ><PencilIcon />
        {m.board_rename_action()}</Item
      ><Separator />
    {/if}
    <Item
      variant="destructive"
      disabled={boards.length <= 1}
      onclick={() => open_delete_dialog(item)}
      ><Trash2Icon />
      {m.common_delete()}</Item
    >
  {/snippet}
  {@const current = item.id === active_board_id && !active_focus_id}
  <ContextMenu.Root>
    <ContextMenu.Trigger class="group/row relative block">
      <button
        type="button"
        class={`relative flex min-h-10 w-full min-w-0 items-center gap-2 rounded-md py-1.5 ps-3 pe-2 text-left text-sm transition-colors ${current ? "bg-muted font-medium text-foreground before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"}`}
        onclick={() => select_board(item.id)}
        aria-current={current ? "page" : undefined}
        title={item.shared_role === "owner"
          ? item.name
          : m.board_shared_name({
              name: item.name,
              permission:
                item.shared_role === "viewer"
                  ? m.board_read_only()
                  : m.board_can_edit(),
            })}
      >
        {#if item.shared_role !== "owner"}
          <UsersIcon class="size-4 shrink-0 text-primary" aria-hidden="true" />
        {/if}
        <span class="min-w-0 flex-1">
          <span class="block truncate">{item.name}</span>
          {#if item.shared_role !== "owner"}
            <span
              class="block truncate text-[11px] font-normal leading-4 text-muted-foreground"
            >
              {m.board_shared()} ·
              {item.shared_role === "viewer"
                ? m.board_read_only()
                : m.board_can_edit()}
            </span>
          {/if}
        </span>
        <!-- The ⋯ button takes the count's place while shown. -->
        <span
          class={`min-w-6 shrink-0 text-end text-xs tabular-nums group-hover/row:invisible group-focus-within/row:invisible ${item.task_count === 0 ? "text-muted-foreground/60" : "text-muted-foreground"}`}
          title={m.task_count({ count: item.task_count })}
          >{formatNumber(item.task_count)}</span
        >
      </button>
      {@render actions_button(m.board_actions({ name: item.name }), items)}
    </ContextMenu.Trigger>
    <ContextMenu.Content>
      {@render items("context")}
    </ContextMenu.Content>
  </ContextMenu.Root>
{/snippet}

{#snippet toggle_button()}
  {@const label = expanded ? m.sidebar_hide() : m.sidebar_show()}
  <Button
    variant="ghost"
    size="icon-sm"
    class="shrink-0 text-muted-foreground"
    onclick={onToggle}
    aria-label={label}
    aria-expanded={expanded}
    aria-controls="board-drawer"
    aria-keyshortcuts="Control+B"
    title={`${label} (Ctrl+B)`}
  >
    {#if expanded}
      <PanelLeftCloseIcon />
    {:else}
      <PanelLeftOpenIcon />
    {/if}
  </Button>
{/snippet}

<!-- The sheet has its own close button, so only the docked sidebar toggles. -->
{#snippet brand(toggle: boolean)}
  <div
    class="flex h-12 shrink-0 items-center gap-1.5 border-b ps-4 pe-2 text-sm font-semibold"
  >
    <LayoutDashboardIcon class="size-4 text-primary" aria-hidden="true" />
    <span class="min-w-0 flex-1 truncate">Cardbe</span>
    {#if toggle}
      {@render toggle_button()}
    {/if}
  </div>
{/snippet}

{#snippet navigation()}
  <div class="shrink-0 px-3 py-3">
    <Button
      variant="outline"
      size="sm"
      class="w-full justify-start gap-2 font-normal text-muted-foreground"
      aria-keyshortcuts="Control+K"
      onclick={quick_switch}
    >
      <SearchIcon />
      <span class="min-w-0 flex-1 truncate text-start"
        >{m.quick_switch_open()}</span
      >
      <Kbd.Group>
        <Kbd.Root>Ctrl</Kbd.Root>
        <Kbd.Root>K</Kbd.Root>
      </Kbd.Group>
    </Button>
  </div>
  <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="vertical">
    <nav class="min-w-0 px-2 pb-2" aria-label={m.board_list_navigation()}>
      <button
        type="button"
        class={`relative flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md py-1.5 ps-3 pe-2 text-left text-sm transition-colors ${active_focus_id === HOME_PAGE_ID ? "bg-muted font-medium text-foreground before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"}`}
        aria-current={active_focus_id === HOME_PAGE_ID ? "page" : undefined}
        aria-keyshortcuts="Control+0"
        onclick={open_home}
      >
        <HouseIcon class="size-4 shrink-0 text-primary" aria-hidden="true" />
        <span class="min-w-0 flex-1 truncate">{m.home_title()}</span>
        <!-- Styled as menu shortcuts; keys stay visible on the selected row. -->
        <span
          class="ms-auto text-xs font-normal tracking-widest text-muted-foreground"
        >
          <Kbd.Group>
            <Kbd.Root
              class={active_focus_id === HOME_PAGE_ID
                ? "bg-background"
                : undefined}>Ctrl</Kbd.Root
            >
            <Kbd.Root
              class={active_focus_id === HOME_PAGE_ID
                ? "bg-background"
                : undefined}>0</Kbd.Root
            >
          </Kbd.Group>
        </span>
      </button>
      <button
        type="button"
        class={`relative flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md py-1.5 ps-3 pe-2 text-left text-sm transition-colors ${active_focus_id === NOTES_PAGE_ID ? "bg-muted font-medium text-foreground before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"}`}
        aria-current={active_focus_id === NOTES_PAGE_ID ? "page" : undefined}
        onclick={open_notes}
      >
        <StickyNoteIcon
          class="size-4 shrink-0 text-primary"
          aria-hidden="true"
        />
        <span class="min-w-0 flex-1 truncate">{m.note_title()}</span>
      </button>
      <Separator class="my-3" />
      <section
        class="rounded-md bg-muted/50 p-1"
        aria-labelledby="drawer-focuses"
      >
        <div class="flex h-8 items-center justify-between ps-2 pe-1">
          <h3
            id="drawer-focuses"
            class="flex items-center gap-1.5 text-xs font-semibold text-foreground"
          >
            <ListFilterIcon class="size-3.5 text-primary" />
            {m.focus_section()}
            <span class="font-normal text-muted-foreground"
              >· {formatNumber(focuses.length)}</span
            >
          </h3>
          {@render add_button(
            m.focus_new(),
            focuses.length >= MAX_FOCUSES,
            () => {
              onCreateFocus();
              sheet_open = false;
            },
          )}
        </div>
        {#if focuses.length > 0}
          <div class="space-y-0.5">
            {#each focuses as view (view.id)}
              {@render focus_row(view)}
            {/each}
          </div>
        {:else}
          <p class="px-2 py-3 text-center text-xs text-muted-foreground">
            {m.focus_list_empty()}
          </p>
        {/if}
      </section>
      <Separator class="my-3" />
      <section aria-labelledby="drawer-boards">
        <div class="flex h-8 items-center justify-between ps-3 pe-1">
          <h3
            id="drawer-boards"
            class="flex items-center gap-1.5 text-xs font-semibold text-foreground"
          >
            <LayoutDashboardIcon class="size-3.5 text-primary" />
            {m.board_list()}
            <span class="font-normal text-muted-foreground"
              >· {formatNumber(boards.length)}</span
            >
          </h3>
          {@render add_button(m.board_new(), false, open_create_dialog)}
        </div>
        {#if boards.length > 0}
          <div class="space-y-0.5">
            {#each boards as item (item.id)}
              {@render board_row(item)}
            {/each}
          </div>
        {:else}
          <p class="px-3 py-8 text-center text-sm text-muted-foreground">
            {m.board_empty()}
          </p>
        {/if}
      </section>
    </nav>
  </ScrollArea>
{/snippet}

{#if compact}
  <Sheet.Root bind:open={sheet_open}>
    <Sheet.Content
      id="board-drawer"
      side="left"
      class="w-72 gap-0 bg-board-sidebar p-0 sm:max-w-72"
    >
      <Sheet.Header class="sr-only">
        <Sheet.Title>{m.board_drawer_title()}</Sheet.Title>
        <Sheet.Description>{m.board_list_navigation()}</Sheet.Description>
      </Sheet.Header>
      {@render brand(false)}
      {@render navigation()}
    </Sheet.Content>
  </Sheet.Root>
{:else if expanded}
  <aside
    id="board-drawer"
    class="relative flex h-full shrink-0 flex-col border-e bg-board-sidebar"
    style:width="min({sidebar_width.value}px, 40%)"
    aria-label={m.board_drawer_title()}
  >
    <button
      type="button"
      class="group absolute inset-y-0 -end-1.5 z-10 flex w-3 cursor-ew-resize justify-center outline-none"
      aria-label={m.sidebar_resize()}
      title={m.common_resize_panel_hint()}
      onpointerdown={(event) => sidebar_width.start(event)}
      onkeydown={(event) => sidebar_width.keydown(event)}
      ondblclick={() => sidebar_width.reset()}
    >
      <span
        class="h-full w-px transition-all group-hover:w-0.5 group-hover:bg-ring group-focus-visible:w-0.5 group-focus-visible:bg-ring"
      ></span>
    </button>
    {@render brand(true)}
    {@render navigation()}
  </aside>
{:else}
  <!-- The collapsed rail keeps Home, Notes and quick switching one click away. -->
  <aside
    id="board-drawer"
    class="flex h-full w-12 shrink-0 flex-col items-center gap-1 border-e bg-board-sidebar"
    aria-label={m.board_drawer_title()}
  >
    <div
      class="mb-1 flex h-12 w-full shrink-0 items-center justify-center border-b"
    >
      {@render toggle_button()}
    </div>
    <Button
      variant={active_focus_id === HOME_PAGE_ID ? "secondary" : "ghost"}
      size="icon-sm"
      aria-label={m.home_title()}
      aria-current={active_focus_id === HOME_PAGE_ID ? "page" : undefined}
      aria-keyshortcuts="Control+0"
      title={`${m.home_title()} (Ctrl+0)`}
      onclick={open_home}
    >
      <HouseIcon />
    </Button>
    <Button
      variant={active_focus_id === NOTES_PAGE_ID ? "secondary" : "ghost"}
      size="icon-sm"
      aria-label={m.note_title()}
      aria-current={active_focus_id === NOTES_PAGE_ID ? "page" : undefined}
      title={m.note_title()}
      onclick={open_notes}
    >
      <StickyNoteIcon />
    </Button>
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={m.quick_switch_open()}
      aria-keyshortcuts="Control+K"
      title={`${m.quick_switch_open()} (Ctrl+K)`}
      onclick={quick_switch}
    >
      <SearchIcon />
    </Button>
  </aside>
{/if}

<Dialog.Root bind:open={create_open} draft={new_name} busy={saving}
  ><Dialog.Content class="sm:max-w-sm"
    ><Dialog.Header
      ><Dialog.Title>{m.board_create()}</Dialog.Title><Dialog.Description
        >{m.board_create_description()}</Dialog.Description
      ></Dialog.Header
    >
    <form
      class="grid gap-4"
      onsubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      <Input
        bind:value={new_name}
        placeholder={m.board_name()}
        aria-label={m.board_name()}
        maxlength={200}
        autofocus
      />
      <div class="flex justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={saving}
          onclick={async () => {
            if (await confirmUnsavedChanges()) {
              create_open = false;
            }
          }}>{m.common_cancel()}</Button
        ><Button type="submit" disabled={!new_name.trim() || saving}
          >{m.common_create()}</Button
        >
      </div>
    </form></Dialog.Content
  ></Dialog.Root
>
<Dialog.Root bind:open={rename_open} draft={rename_name} busy={saving}
  ><Dialog.Content class="sm:max-w-sm"
    ><Dialog.Header
      ><Dialog.Title>{m.board_rename()}</Dialog.Title><Dialog.Description
        >{m.board_rename_description()}</Dialog.Description
      ></Dialog.Header
    >
    <form
      class="grid gap-4"
      onsubmit={(event) => {
        event.preventDefault();
        void rename();
      }}
    >
      <Input
        bind:value={rename_name}
        aria-label={m.board_name()}
        maxlength={200}
        autofocus
      />
      <div class="flex justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={saving}
          onclick={async () => {
            if (await confirmUnsavedChanges()) {
              rename_open = false;
            }
          }}>{m.common_cancel()}</Button
        ><Button type="submit" disabled={!rename_name.trim() || saving}
          >{m.common_save()}</Button
        >
      </div>
    </form></Dialog.Content
  ></Dialog.Root
>
<AlertDialog.Root bind:open={delete_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title
        >{m.board_delete_named_confirm({
          name: selected_board?.name ?? m.board_title(),
        })}</AlertDialog.Title
      ><AlertDialog.Description
        >{archived_count
          ? m.board_delete_description_with_archives({
              count: archived_count,
            })
          : m.board_delete_description()}</AlertDialog.Description
      ></AlertDialog.Header
    ><AlertDialog.Footer
      ><AlertDialog.Cancel disabled={saving}
        >{m.common_cancel()}</AlertDialog.Cancel
      ><AlertDialog.Action
        variant="destructive"
        disabled={saving}
        onclick={() => void remove()}>{m.common_delete()}</AlertDialog.Action
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
<AlertDialog.Root bind:open={delete_view_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title
        >{m.focus_delete_confirm({
          name: selected_view?.name ?? "",
        })}</AlertDialog.Title
      ><AlertDialog.Description
        >{m.focus_delete_description()}</AlertDialog.Description
      ></AlertDialog.Header
    ><AlertDialog.Footer
      ><AlertDialog.Cancel disabled={saving}
        >{m.common_cancel()}</AlertDialog.Cancel
      ><AlertDialog.Action
        variant="destructive"
        disabled={saving}
        onclick={() => void remove_view()}
        >{m.common_delete()}</AlertDialog.Action
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
