<script lang="ts">
import PencilIcon from "@lucide/svelte/icons/pencil";
import PlusIcon from "@lucide/svelte/icons/plus";
import SearchIcon from "@lucide/svelte/icons/search";
import Trash2Icon from "@lucide/svelte/icons/trash-2";
import UsersIcon from "@lucide/svelte/icons/users";
import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
import { Button } from "$lib/components/ui/button/index.js";
import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
import * as Dialog from "$lib/components/ui/dialog/index.js";
import { Input } from "$lib/components/ui/input/index.js";
import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
import type { BoardRole } from "../../board.svelte";

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
  onDelete,
}: {
  boards: BoardItem[];
  active_board_id: number | null;
  open: boolean;
  onSwitch: (id: number) => void;
  onCreate: (name: string) => Promise<boolean>;
  onRename: (id: number, name: string) => Promise<boolean>;
  onDelete: (id: number) => Promise<boolean>;
} = $props();

let create_open = $state(false);
let rename_open = $state(false);
let delete_open = $state(false);
let new_name = $state("");
let rename_name = $state("");
let search = $state("");
let selected_board = $state<BoardItem | null>(null);
let saving = $state(false);
const filtered_boards = $derived(
  boards.filter((item) =>
    item.name.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()),
  ),
);
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
  delete_open = true;
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
    !delete_open
  )
    open = false;
}
async function create() {
  if (!new_name.trim() || saving) return;
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
  if (!selected_board || !rename_name.trim() || saving) return;
  saving = true;
  try {
    if (await onRename(selected_board.id, rename_name.trim()))
      rename_open = false;
  } finally {
    saving = false;
  }
}
async function remove() {
  if (!selected_board || saving) return;
  saving = true;
  try {
    if (await onDelete(selected_board.id)) delete_open = false;
  } finally {
    saving = false;
  }
}
</script>

<svelte:window onkeydown={handle_keydown} />

{#if open}
  <div class="fixed inset-0 z-30" aria-label="Board navigation">
    <button
      class="absolute inset-0 cursor-default bg-black/10"
      aria-label="Close boards"
      onclick={() => open = false}
    ></button>
    <section
      id="board-drawer"
      class="fixed top-12 left-4 z-40 flex h-[min(60dvh,calc(100dvh-4rem))] w-64 flex-col overflow-hidden rounded-md border bg-background shadow-xl motion-safe:animate-in motion-safe:fade-in-0 motion-safe:zoom-in-95 motion-safe:duration-150"
      role="navigation"
      aria-label="Boards"
    >
      <header class="flex h-11 shrink-0 items-center border-b px-3">
        <span class="text-sm font-semibold"
          >Boards
          <span class="font-normal text-muted-foreground"
            >· {boards.length}</span
          ></span
        >
      </header>
      <div class="relative shrink-0 px-3 py-3">
        <SearchIcon
          class="pointer-events-none absolute top-5.5 left-5.5 size-4 text-muted-foreground"
        />
        <Input
          bind:value={search}
          class="h-8 pl-8"
          placeholder="Search boards"
          aria-label="Search boards"
        />
      </div>
      <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="vertical">
        <nav class="min-w-0 px-2 py-2" aria-label="Board list">
          {#if filtered_boards.length > 0}
            <div class="space-y-0.5">
              {#each filtered_boards as item (item.id)}
                <ContextMenu.Root>
                  <ContextMenu.Trigger>
                    <button
                      type="button"
                      class={`relative flex min-h-10 w-full min-w-0 items-center gap-2 rounded-md px-3 py-1.5 text-left text-sm transition-colors ${item.id === active_board_id ? "bg-muted font-medium text-foreground before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-primary" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"}`}
                      onclick={() => select_board(item.id)}
                      aria-current={item.id === active_board_id ? "page" : undefined}
                      title={item.shared_role === "owner" ? item.name : `${item.name} — Shared with me, ${item.shared_role === "viewer" ? "read only" : "can edit"}`}
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
                            Shared with me ·
                            {item.shared_role === "viewer" ? "Read only" : "Can edit"}
                          </span>
                        {/if}
                      </span>
                      <span
                        class={`shrink-0 text-xs tabular-nums ${item.task_count === 0 ? "text-muted-foreground/60" : "text-muted-foreground"}`}
                        >{item.task_count}</span
                      >
                    </button>
                  </ContextMenu.Trigger>
                  <ContextMenu.Content>
                    {#if item.shared_role === "owner"}
                      <ContextMenu.Item onclick={() => open_rename_dialog(item)}
                        ><PencilIcon />
                        Rename</ContextMenu.Item
                      ><ContextMenu.Separator />
                    {/if}
                    <ContextMenu.Item
                      class="text-destructive focus:text-destructive"
                      disabled={boards.length <= 1}
                      onclick={() => open_delete_dialog(item)}
                      ><Trash2Icon />
                      Delete</ContextMenu.Item
                    >
                  </ContextMenu.Content>
                </ContextMenu.Root>
              {/each}
            </div>
          {:else}
            <p class="px-3 py-8 text-center text-sm text-muted-foreground">
              No boards found
            </p>
          {/if}
        </nav>
      </ScrollArea>
      <footer class="shrink-0 border-t p-2">
        <Button
          variant="ghost"
          class="w-full justify-start"
          onclick={open_create_dialog}
          aria-label="Create board"
        >
          <PlusIcon />
          New board
        </Button>
      </footer>
    </section>
  </div>
{/if}

<Dialog.Root bind:open={create_open}
  ><Dialog.Content class="sm:max-w-sm"
    ><Dialog.Header
      ><Dialog.Title>Create board</Dialog.Title
      ><Dialog.Description
        >Give the new board a name.</Dialog.Description
      ></Dialog.Header
    >
    <form
      class="grid gap-4"
      onsubmit={(event) => { event.preventDefault(); void create(); }}
    >
      <Input
        bind:value={new_name}
        placeholder="Board name"
        aria-label="Board name"
        maxlength={200}
        autofocus
      />
      <div class="flex justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          onclick={() => create_open = false}
          >Cancel</Button
        ><Button type="submit" disabled={!new_name.trim() || saving}
          >Create</Button
        >
      </div>
    </form></Dialog.Content
  ></Dialog.Root
>
<Dialog.Root bind:open={rename_open}
  ><Dialog.Content class="sm:max-w-sm"
    ><Dialog.Header
      ><Dialog.Title>Rename board</Dialog.Title
      ><Dialog.Description
        >Choose a new name for this board.</Dialog.Description
      ></Dialog.Header
    >
    <form
      class="grid gap-4"
      onsubmit={(event) => { event.preventDefault(); void rename(); }}
    >
      <Input
        bind:value={rename_name}
        aria-label="Board name"
        maxlength={200}
        autofocus
      />
      <div class="flex justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          onclick={() => rename_open = false}
          >Cancel</Button
        ><Button type="submit" disabled={!rename_name.trim() || saving}
          >Save</Button
        >
      </div>
    </form></Dialog.Content
  ></Dialog.Root
>
<AlertDialog.Root bind:open={delete_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title
        >Delete {selected_board?.name ?? "board"}?</AlertDialog.Title
      ><AlertDialog.Description
        >This permanently deletes the board and all of its contents. This action
        cannot be undone.</AlertDialog.Description
      ></AlertDialog.Header
    ><AlertDialog.Footer
      ><AlertDialog.Cancel disabled={saving}>Cancel</AlertDialog.Cancel
      ><AlertDialog.Action disabled={saving} onclick={() => void remove()}
        >Delete</AlertDialog.Action
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
