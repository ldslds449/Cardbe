<script lang="ts">
  import { confirmUnsavedChanges } from "$lib/unsaved-changes";
  import { formatNumber } from "$lib/i18n";
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import SearchIcon from "@lucide/svelte/icons/search";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import UsersIcon from "@lucide/svelte/icons/users";
  import ListFilterIcon from "@lucide/svelte/icons/list-filter";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import type { BoardRole } from "../../board.svelte";
  import { MAX_SMART_VIEWS, type SmartView } from "../../type/smart-view";

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
    open = $bindable(),
    onSwitch,
    onCreate,
    onRename,
    onLoadArchivedCount,
    onDelete,
    smart_views,
    active_smart_view_id,
    onOpenSmartView,
    onCreateSmartView,
    onEditSmartView,
    onDeleteSmartView,
  }: {
    boards: BoardItem[];
    active_board_id: number | null;
    open: boolean;
    onSwitch: (id: number) => void;
    onCreate: (name: string) => Promise<boolean>;
    onRename: (id: number, name: string) => Promise<boolean>;
    onLoadArchivedCount: (id: number) => Promise<number | null>;
    onDelete: (id: number) => Promise<boolean>;
    smart_views: SmartView[];
    /** The smart view shown instead of the active board, if any. */
    active_smart_view_id: string | null;
    onOpenSmartView: (id: string) => void;
    onCreateSmartView: () => void;
    onEditSmartView: (id: string) => void;
    onDeleteSmartView: (id: string) => Promise<boolean>;
  } = $props();

  let create_open = $state(false);
  let rename_open = $state(false);
  let delete_open = $state(false);
  let new_name = $state("");
  let rename_name = $state("");
  let search = $state("");
  let selected_board = $state<BoardItem | null>(null);
  let archived_count = $state<number | null>(null);
  let saving = $state(false);
  const filtered_boards = $derived(
    boards.filter((item) =>
      item.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()),
    ),
  );
  const filtered_smart_views = $derived(
    smart_views.filter((view) =>
      view.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()),
    ),
  );
  let delete_view_open = $state(false);
  let selected_view = $state<SmartView | null>(null);
  function open_delete_view_dialog(view: SmartView) {
    selected_view = view;
    delete_view_open = true;
  }
  async function remove_view() {
    if (!selected_view || saving) {
      return;
    }
    saving = true;
    try {
      if (await onDeleteSmartView(selected_view.id)) {
        delete_view_open = false;
      }
    } finally {
      saving = false;
    }
  }
  function select_view(id: string) {
    onOpenSmartView(id);
    open = false;
  }
  function open_create_dialog() {
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
    open = false;
  }
  function handle_keydown(event: KeyboardEvent) {
    if (
      event.key === "Escape" &&
      open &&
      !create_open &&
      !rename_open &&
      !delete_open &&
      !delete_view_open
    ) {
      open = false;
    }
  }
  async function create() {
    if (!new_name.trim() || saving) {
      return;
    }
    saving = true;
    try {
      if (await onCreate(new_name.trim())) {
        create_open = false;
        open = false;
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

<svelte:window onkeydown={handle_keydown} />

{#if open}
  <div class="fixed inset-0 z-30" aria-label={m.board_navigation()}>
    <button
      class="absolute inset-0 cursor-default bg-black/10"
      aria-label={m.board_close()}
      onclick={() => (open = false)}
    ></button>
    <section
      id="board-drawer"
      class="fixed top-12 left-4 z-40 flex h-[min(60dvh,calc(100dvh-4rem))] w-64 flex-col overflow-hidden rounded-md border bg-board-sidebar shadow-xl motion-safe:animate-in motion-safe:fade-in-0 motion-safe:zoom-in-95 motion-safe:duration-150"
      role="navigation"
      aria-label={m.board_drawer_title()}
    >
      <header class="flex h-11 shrink-0 items-center border-b px-3">
        <span class="text-sm font-semibold">{m.board_drawer_title()}</span>
      </header>
      <div class="relative shrink-0 px-3 py-3">
        <SearchIcon
          class="pointer-events-none absolute top-5.5 left-5.5 size-4 text-muted-foreground"
        />
        <Input
          bind:value={search}
          class="h-8 pl-8"
          placeholder={m.board_drawer_search()}
          aria-label={m.board_drawer_search()}
        />
      </div>
      <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="vertical">
        <nav class="min-w-0 px-2 py-2" aria-label={m.board_list_navigation()}>
          <section
            class="mb-3 rounded-md bg-muted/50 p-1"
            aria-labelledby="drawer-smart-views"
          >
            <div class="flex h-8 items-center justify-between ps-2 pe-1">
              <h3
                id="drawer-smart-views"
                class="flex items-center gap-1.5 text-xs font-semibold text-foreground"
              >
                <ListFilterIcon class="size-3.5 text-primary" />
                {m.smart_view_section()}
                <span class="font-normal text-muted-foreground"
                  >· {formatNumber(smart_views.length)}</span
                >
              </h3>
              <Button
                variant="ghost"
                size="icon-xs"
                class="text-muted-foreground"
                aria-label={m.smart_view_new()}
                title={m.smart_view_new()}
                disabled={smart_views.length >= MAX_SMART_VIEWS}
                onclick={() => {
                  onCreateSmartView();
                  open = false;
                }}
              >
                <PlusIcon />
              </Button>
            </div>
            {#if filtered_smart_views.length > 0}
              <div class="space-y-0.5">
                {#each filtered_smart_views as view (view.id)}
                  <ContextMenu.Root>
                    <ContextMenu.Trigger>
                      <button
                        type="button"
                        class={`relative flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors ${view.id === active_smart_view_id ? "bg-background font-medium text-foreground shadow-xs before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary" : "text-muted-foreground hover:bg-background/70 hover:text-foreground"}`}
                        onclick={() => select_view(view.id)}
                        aria-current={view.id === active_smart_view_id
                          ? "page"
                          : undefined}
                        title={view.name}
                      >
                        <ListFilterIcon
                          class="size-4 shrink-0 text-primary"
                          aria-hidden="true"
                        />
                        <span class="min-w-0 flex-1 truncate">{view.name}</span>
                      </button>
                    </ContextMenu.Trigger>
                    <ContextMenu.Content>
                      <ContextMenu.Item
                        onclick={() => {
                          onEditSmartView(view.id);
                          open = false;
                        }}
                        ><PencilIcon />
                        {m.common_edit()}</ContextMenu.Item
                      ><ContextMenu.Separator />
                      <ContextMenu.Item
                        variant="destructive"
                        onclick={() => open_delete_view_dialog(view)}
                        ><Trash2Icon />
                        {m.common_delete()}</ContextMenu.Item
                      >
                    </ContextMenu.Content>
                  </ContextMenu.Root>
                {/each}
              </div>
            {:else if smart_views.length === 0}
              <p class="px-2 py-2 text-xs text-muted-foreground">
                {m.smart_view_none()}
              </p>
            {/if}
          </section>
          <h3
            class="flex h-8 items-center gap-1.5 px-3 text-xs font-semibold text-foreground"
          >
            <LayoutDashboardIcon class="size-3.5 text-muted-foreground" />
            {m.board_list()}
            <span class="font-normal text-muted-foreground"
              >· {formatNumber(boards.length)}</span
            >
          </h3>
          {#if filtered_boards.length > 0}
            <div class="space-y-0.5">
              {#each filtered_boards as item (item.id)}
                <ContextMenu.Root>
                  <ContextMenu.Trigger>
                    <button
                      type="button"
                      class={`relative flex min-h-10 w-full min-w-0 items-center gap-2 rounded-md px-3 py-1.5 text-left text-sm transition-colors ${item.id === active_board_id && !active_smart_view_id ? "bg-muted font-medium text-foreground before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"}`}
                      onclick={() => select_board(item.id)}
                      aria-current={item.id === active_board_id &&
                      !active_smart_view_id
                        ? "page"
                        : undefined}
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
                        <UsersIcon
                          class="size-4 shrink-0 text-primary"
                          aria-hidden="true"
                        />
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
                      <span
                        class={`shrink-0 text-xs tabular-nums ${item.task_count === 0 ? "text-muted-foreground/60" : "text-muted-foreground"}`}
                        title={m.task_count({ count: item.task_count })}
                        >{item.task_count}</span
                      >
                    </button>
                  </ContextMenu.Trigger>
                  <ContextMenu.Content>
                    {#if item.shared_role === "owner"}
                      <ContextMenu.Item onclick={() => open_rename_dialog(item)}
                        ><PencilIcon />
                        {m.board_rename_action()}</ContextMenu.Item
                      ><ContextMenu.Separator />
                    {/if}
                    <ContextMenu.Item
                      variant="destructive"
                      disabled={boards.length <= 1}
                      onclick={() => open_delete_dialog(item)}
                      ><Trash2Icon />
                      {m.common_delete()}</ContextMenu.Item
                    >
                  </ContextMenu.Content>
                </ContextMenu.Root>
              {/each}
            </div>
          {:else}
            <p class="px-3 py-8 text-center text-sm text-muted-foreground">
              {m.board_empty()}
            </p>
          {/if}
        </nav>
      </ScrollArea>
      <footer class="shrink-0 border-t p-2">
        <Button
          variant="ghost"
          class="w-full justify-start"
          onclick={open_create_dialog}
          aria-label={m.board_create()}
        >
          <PlusIcon />
          {m.board_new()}
        </Button>
      </footer>
    </section>
  </div>
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
        >{m.smart_view_delete_confirm({
          name: selected_view?.name ?? "",
        })}</AlertDialog.Title
      ><AlertDialog.Description
        >{m.smart_view_delete_description()}</AlertDialog.Description
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
