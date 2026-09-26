<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
import LinkIcon from "@lucide/svelte/icons/link";
import UsersIcon from "@lucide/svelte/icons/users";
import Settings2Icon from "@lucide/svelte/icons/settings-2";
import ClipboardIcon from "@lucide/svelte/icons/clipboard";
import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
import { onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge/index.js";
import { Button } from "$lib/components/ui/button/index.js";
import * as ButtonGroup from "$lib/components/ui/button-group/index.js";
import * as Card from "$lib/components/ui/card/index.js";
import * as Collapsible from "$lib/components/ui/collapsible/index.js";
import * as Dialog from "$lib/components/ui/dialog/index.js";
import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
import { Input } from "$lib/components/ui/input/index.js";
import { Label } from "$lib/components/ui/label/index.js";
import * as Select from "$lib/components/ui/select/index.js";
import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
import { Switch } from "$lib/components/ui/switch/index.js";
import type { BoardStore } from "../../board.svelte";
import { IrohDeviceStatus, type IrohInvite } from "../../type/iroh-share";

type Flow = "home" | "share" | "join" | "manage";
type Invite = IrohInvite;
type Generated = {
  ticket: string;
  qr: string;
  board_name: string;
  permission: "viewer" | "editor";
};
type InviteAccess = { ticket: string; qr_svg: string };
let {
  open = $bindable(),
  board,
  show_requests = false,
  show_received = false,
  onRequestsChanged = () => {},
}: {
  open: boolean;
  board: BoardStore;
  show_requests?: boolean;
  show_received?: boolean;
  onRequestsChanged?: () => void;
} = $props();
let flow = $state<Flow>("home"),
  ticket = $state(""),
  permission = $state<"viewer" | "editor">("viewer"),
  selected_board_id = $state<number | null>(null);
let generated = $state<Generated | null>(null),
  invites = $state<Invite[]>([]),
  joining = $state(false),
  creating = $state(false),
  pending_delete = $state<Invite | null>(null),
  delete_confirm_open = $state(false);
let visible_qr_id = $state<string | null>(null),
  invite_access = $state<Record<string, InviteAccess>>({}),
  loading_access_id = $state<string | null>(null);
let manage_view = $state<"mine" | "received">("mine");
let host_error = $state<string | null>(null);
let waiting_device_id = $state<string | null>(null);
let checking_approval = false;
let wait_generation = 0;
let approval_candidate = $state<{ invite: Invite; node_id: string } | null>(
  null,
);
let approval_open = $state(false);
let revoke_candidate = $state<{ invite: Invite; node_id: string } | null>(null);
let revoke_open = $state(false);
let received_pending = $state<Record<number, string>>({});
let received_waiting_id = $state<number | null>(null);
const sync_time = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});
let received_requesting = new Set<number>();
let received_retry_request = new Set<number>();
let stopped_waiting = new Set<number>();
let expanded_devices = $state<Record<string, boolean>>({});
const pending_count = $derived(
  invites.reduce(
    (count, invite) =>
      count +
      invite.devices.filter(
        (device) => device.status === IrohDeviceStatus.Pending,
      ).length,
    0,
  ),
);
onMount(() => {
  const timer = window.setInterval(() => {
    if (open && flow === "manage") void load(true);
    if (
      open &&
      flow === "join" &&
      waiting_device_id &&
      !joining &&
      !checking_approval
    )
      void check_approval();
    if (open)
      for (const id of Object.keys(received_pending))
        void request_received_access(Number(id), false);
  }, 5000);
  return () => window.clearInterval(timer);
});
$effect(() => {
  if (open && show_requests) {
    flow = "manage";
    manage_view = "mine";
    void load();
  }
});
$effect(() => {
  if (open && show_received) {
    flow = "manage";
    manage_view = "received";
    void load();
  }
});
const active = $derived(
  board.boards.find((item) => item.id === board.active_board_id),
);
const owned_boards = $derived(
  board.boards.filter((item) => item.shared_role === "owner"),
);
const selected_board = $derived(
  owned_boards.find((item) => item.id === selected_board_id),
);
const received_boards = $derived(
  board.boards.filter((item) => item.shared_role !== "owner"),
);
$effect(() => {
  if (selected_board_id === null)
    selected_board_id =
      active?.shared_role === "owner"
        ? active.id
        : (owned_boards[0]?.id ?? null);
});
$effect(() => {
  if (open) return;
  flow = "home";
  ticket = "";
  board.iroh_last_error = "";
  permission = "viewer";
  generated = null;
  waiting_device_id = null;
  received_waiting_id = null;
  manage_view = "mine";
  visible_qr_id = null;
});
function invalidate() {
  generated = null;
}
function open_join() {
  ticket = "";
  waiting_device_id = null;
  board.iroh_last_error = "";
  flow = "join";
}
async function create() {
  if (selected_board_id === null) return;
  creating = true;
  try {
    const created = await board.create_iroh_invite(
      selected_board_id,
      permission,
    );
    generated = {
      ticket: created,
      qr: await invoke<string>("iroh_invite_qr_svg", { ticket: created }),
      board_name: selected_board?.name ?? "Board",
      permission,
    };
    let copied = false;
    try {
      await navigator.clipboard.writeText(created);
      copied = true;
    } catch {
      /* The visible link can still be copied later. */
    }
    await load();
    toast.success(copied ? "Invitation copied" : "Invitation created");
  } catch (e) {
    toast.error(e instanceof Error ? e.message : "Couldn't create invitation");
  } finally {
    creating = false;
  }
}
async function copy() {
  if (!generated) return;
  try {
    await navigator.clipboard.writeText(generated.ticket);
    toast.success("Invitation copied");
  } catch {
    toast.error("Couldn't copy invitation");
  }
}
async function join(silent = false) {
  const generation = wait_generation;
  joining = true;
  board.iroh_last_error = "";
  try {
    const joined = await board.join_iroh_invite(ticket, silent);
    if (generation !== wait_generation) {
      board.iroh_last_error = "";
      return;
    }
    if (joined) {
      open = false;
      return;
    }
    if (board.iroh_last_error.startsWith("APPROVAL_REQUIRED:")) {
      waiting_device_id = board.iroh_last_error.slice(
        "APPROVAL_REQUIRED:".length,
      );
      board.iroh_last_error = "";
    } else if (
      silent &&
      board.iroh_last_error.startsWith("Access was declined or revoked")
    )
      waiting_device_id = null;
  } finally {
    joining = false;
  }
}
async function check_approval() {
  checking_approval = true;
  try {
    await join(true);
  } finally {
    checking_approval = false;
  }
}
async function load(silent = false) {
  try {
    invites = await invoke<Invite[]>("list_iroh_invites");
    host_error = await invoke<string | null>("iroh_host_error");
    onRequestsChanged();
  } catch (e) {
    if (!silent)
      toast.error(e instanceof Error ? e.message : "Couldn't load invitations");
  }
}
async function approve_device(
  invite: Invite,
  node_id: string,
  approved: boolean,
) {
  try {
    await invoke("set_iroh_device_approved", {
      inviteId: invite.invite_id,
      nodeId: node_id,
      approved,
    });
    approval_open = false;
    await load();
    toast.success(
      approved
        ? "Device approved"
        : invite.devices.find((device) => device.node_id === node_id)
              ?.status === IrohDeviceStatus.Pending
          ? "Request rejected"
          : "Device access revoked",
    );
    return true;
  } catch (e) {
    toast.error(e instanceof Error ? e.message : "Couldn't update device");
    return false;
  }
}
async function revoke_device() {
  if (!revoke_candidate) return;
  if (
    await approve_device(
      revoke_candidate.invite,
      revoke_candidate.node_id,
      false,
    )
  ) {
    revoke_open = false;
    revoke_candidate = null;
  }
}
async function request_received_access(
  board_id: number,
  request_approval: boolean,
) {
  if (request_approval) {
    stopped_waiting.delete(board_id);
    received_waiting_id = board_id;
  }
  if (received_requesting.has(board_id)) {
    if (request_approval) received_retry_request.add(board_id);
    return;
  }
  received_requesting.add(board_id);
  try {
    const [approved, device_id] = await invoke<[boolean, string]>(
      "request_iroh_board_access",
      { boardId: board_id, requestApproval: request_approval },
    );
    if (stopped_waiting.has(board_id)) return;
    if (approved) {
      delete received_pending[board_id];
      received_waiting_id = null;
      board.iroh_access_removed = {
        ...board.iroh_access_removed,
        [board_id]: false,
      };
      if (await board.sync_iroh_board(board_id, true))
        toast.success("Access restored");
    } else {
      received_pending[board_id] = device_id;
      if (request_approval) toast.success("Access request sent");
    }
  } catch (e) {
    if (
      !request_approval &&
      (e instanceof Error ? e.message : String(e)).startsWith(
        "Access was declined or revoked",
      )
    )
      delete received_pending[board_id];
    else if (request_approval) {
      received_waiting_id = null;
      toast.error(e instanceof Error ? e.message : String(e));
    }
  } finally {
    received_requesting.delete(board_id);
    if (
      received_retry_request.delete(board_id) &&
      !stopped_waiting.has(board_id)
    )
      void request_received_access(board_id, true);
  }
}
async function copy_device_id(id: string) {
  try {
    await navigator.clipboard.writeText(id);
    toast.success("Device ID copied");
  } catch {
    toast.error("Couldn't copy Device ID");
  }
}
async function update(
  invite: Invite,
  change: { permission?: "viewer" | "editor"; enabled?: boolean },
) {
  try {
    await invoke("update_iroh_invite", {
      inviteId: invite.invite_id,
      ...change,
    });
    visible_qr_id = null;
    const next = { ...invite_access };
    delete next[invite.invite_id];
    invite_access = next;
    await load();
    if (change.enabled === true && host_error)
      toast.error("Invitation saved, but sharing is offline");
    else toast.success("Invitation updated");
  } catch (e) {
    await load();
    toast.error(e instanceof Error ? e.message : "Couldn't update invitation");
  }
}
async function remove() {
  if (!pending_delete) return;
  try {
    await invoke("delete_iroh_invite", { inviteId: pending_delete.invite_id });
    await load();
    toast.success("Invitation deleted");
  } catch (e) {
    toast.error(e instanceof Error ? e.message : "Couldn't delete invitation");
  } finally {
    pending_delete = null;
    delete_confirm_open = false;
  }
}
async function access(invite: Invite): Promise<InviteAccess | null> {
  if (invite_access[invite.invite_id]) return invite_access[invite.invite_id];
  loading_access_id = invite.invite_id;
  try {
    const value = await invoke<InviteAccess>("get_iroh_invite_access", {
      inviteId: invite.invite_id,
    });
    invite_access = { ...invite_access, [invite.invite_id]: value };
    return value;
  } catch (e) {
    toast.error(e instanceof Error ? e.message : "Couldn't load invitation");
    return null;
  } finally {
    loading_access_id = null;
  }
}
async function copy_invite(invite: Invite) {
  const value = await access(invite);
  if (!value) return;
  try {
    await navigator.clipboard.writeText(value.ticket);
    toast.success("Invitation copied");
  } catch {
    toast.error("Couldn't copy invitation");
  }
}
async function toggle_qr(invite: Invite) {
  if (visible_qr_id === invite.invite_id) {
    visible_qr_id = null;
    return;
  }
  if (await access(invite)) visible_qr_id = invite.invite_id;
}
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    showCloseButton={!waiting_device_id && received_waiting_id === null}
    interactOutsideBehavior={waiting_device_id || received_waiting_id !== null ? "ignore" : "close"}
    onEscapeKeydown={(event) => { if (waiting_device_id || received_waiting_id !== null) event.preventDefault(); }}
    class={flow === "manage" && received_waiting_id === null
    ? "flex h-[min(42rem,calc(100dvh-2rem))] w-[calc(100vw-2rem)] flex-col overflow-hidden sm:max-w-xl"
    : "max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] overflow-y-auto sm:max-w-xl"}
  >
    <Dialog.Header>
      <Dialog.Title
        >{received_waiting_id !== null ? "Waiting for approval" : flow === "home" ? "Board sharing" : flow === "share" ? "Share a board" : flow === "join" ? "Join a shared board" : "Manage board sharing"}</Dialog.Title
      >
      {#if flow === "home"}
        <Dialog.Description
          >Collaborate in Cardbe with read-only or editing
          access.</Dialog.Description
        >
      {:else if flow === "manage" && received_waiting_id === null}
        <Dialog.Description
          >Manage boards you share and boards shared with
          you.</Dialog.Description
        >
      {/if}
    </Dialog.Header>
    <div
      class={flow === "manage" && received_waiting_id === null
      ? "flex min-h-0 flex-1 flex-col gap-4"
      : flow === "home"
        ? "grid gap-2"
        : "grid gap-4"}
    >
      {#if flow !== "home" && !waiting_device_id && received_waiting_id === null}
        <Button variant="ghost" class="w-fit" onclick={() => flow = "home"}
          ><ArrowLeftIcon />
          Back</Button
        >
      {/if}
      {#if received_waiting_id !== null || waiting_device_id}
        {@const device_id = waiting_device_id ?? received_pending[received_waiting_id!]}
        <Card.Root
          class="items-center gap-4 border-primary/25 bg-primary/5 px-5 py-7 text-center shadow-none"
          role="status"
          ><div
            class="flex size-12 items-center justify-center rounded-full bg-primary/10 text-primary"
          >
            <LoaderCircleIcon class="size-6 animate-spin" />
          </div>
          <div class="space-y-1">
            <Card.Title>Waiting for approval</Card.Title
            ><Card.Description
              >The owner needs to approve this device. This screen checks
              automatically.</Card.Description
            >
          </div>
          {#if device_id}
            <div class="w-full rounded-lg border bg-background p-3 text-left">
              <p class="mb-2 text-xs font-medium text-muted-foreground">
                Your device ID · compare this with the owner
              </p>
              <code class="block break-all text-sm leading-relaxed select-all"
                >{device_id}</code
              >
            </div>
            <Button
              variant="outline"
              onclick={() => void copy_device_id(device_id)}
              ><ClipboardIcon />
              Copy device ID</Button
            >
          {/if}</Card.Root
        >
        <Button
          variant="outline"
          onclick={() => { if (received_waiting_id !== null) { stopped_waiting.add(received_waiting_id); delete received_pending[received_waiting_id]; received_waiting_id = null; } else { wait_generation++; waiting_device_id = null; board.iroh_last_error = ""; } }}
          >Cancel waiting</Button
        >
      {:else if flow === "home"}
        <div class="grid gap-3 sm:grid-cols-2">
          <button
            class="rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onclick={() => flow = "share"}
          >
            <Card.Root
              class="h-full gap-2 p-4 transition-colors hover:bg-accent"
              ><UsersIcon class="size-5 text-primary" />
              <Card.Title class="text-base">Share a board</Card.Title
              ><Card.Description
                >Choose a board and give people viewing or editing
                access.</Card.Description
              ></Card.Root
            >
          </button><button
            class="rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onclick={open_join}
          >
            <Card.Root
              class="h-full gap-2 p-4 transition-colors hover:bg-accent"
              ><LinkIcon class="size-5 text-primary" />
              <Card.Title class="text-base">Join a shared board</Card.Title
              ><Card.Description
                >Paste an invitation link to join.</Card.Description
              ></Card.Root
            >
          </button>
        </div>
        <Button
          variant="outline"
          onclick={() => { void load(); flow = "manage"; }}
          ><Settings2Icon />
          Manage sharing</Button
        >
      {:else if flow === "share"}
        <div class="grid gap-4">
          <label class="grid gap-2 text-sm font-medium"
            >Board<Select.Root
              type="single"
              value={selected_board_id?.toString() ?? ""}
              onValueChange={(v) => { selected_board_id = v ? Number(v) : null; invalidate(); }}
              ><Select.Trigger class="w-full"
                >{selected_board?.name ?? "Choose a board"}</Select.Trigger
              ><Select.Content
                >{#each owned_boards as item}
                  <Select.Item value={item.id.toString()}
                    >{item.name}</Select.Item
                  >
                {/each}</Select.Content
              ></Select.Root
            ></label
          ><label class="grid gap-2 text-sm font-medium"
            >Permission<Select.Root
              type="single"
              value={permission}
              onValueChange={(v) => { permission = v as "viewer" | "editor"; invalidate(); }}
              ><Select.Trigger class="w-full"
                >{permission === "viewer" ? "Read only" : "Can edit"}</Select.Trigger
              ><Select.Content
                ><Select.Item value="viewer">Read only</Select.Item
                ><Select.Item value="editor"
                  >Can edit</Select.Item
                ></Select.Content
              ></Select.Root
            ></label
          >
          {#if generated}
            <Card.Root class="gap-3 bg-muted/30 p-4 shadow-none"
              ><div class="flex items-center justify-between gap-2">
                <Card.Title class="text-base">{generated.board_name}</Card.Title
                ><Badge variant="secondary"
                  >{generated.permission === "viewer" ? "Read only" : "Can edit"}</Badge
                >
              </div>
              <div
                class="qr mx-auto aspect-square w-full max-w-56 overflow-hidden rounded-md bg-white p-2"
              >
                {@html generated.qr}
              </div>
              <Button variant="outline" onclick={() => void copy()}
                >Copy invitation link</Button
              ><Card.Description
                >Approve each device in Manage sharing before it can join.
                Create another invitation to manage access
                separately.</Card.Description
              ></Card.Root
            >
          {/if}
          <Button
            onclick={() => void create()}
            disabled={creating || selected_board_id === null}
            >{creating ? "Creating..." : generated ? "Create new invitation" : "Create invitation"}</Button
          >
        </div>
      {:else if flow === "join"}
        <Card.Root class="gap-4 border-primary/20 bg-muted/20 p-5 shadow-none"
          ><div class="flex items-center gap-3">
            <span
              class="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary"
              ><LinkIcon class="size-5" /></span
            >
            <div>
              <Card.Title class="text-base">Invitation link</Card.Title
              ><Card.Description
                >Paste the link from the board owner.</Card.Description
              >
            </div>
          </div>
          <Input
            bind:value={ticket}
            placeholder="cardbe://share/..."
            aria-label="Invitation link"
            oninput={() => board.iroh_last_error = ""}
          /></Card.Root
        >
        {#if board.iroh_last_error}
          <p
            class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
            role="alert"
          >
            {board.iroh_last_error}
          </p>
        {/if}
        <Button onclick={() => void join()} disabled={!ticket.trim() || joining}
          >{joining ? "Connecting..." : "Request access"}</Button
        >
      {:else}
        <ButtonGroup.Root
          class="grid grid-cols-2"
          role="tablist"
          aria-label="Sharing management views"
        >
          <Button
            variant={manage_view === "mine" ? "secondary" : "outline"}
            role="tab"
            aria-selected={manage_view === "mine"}
            onclick={() => manage_view = "mine"}
            >Shared by me ({invites.length})</Button
          >
          <Button
            variant={manage_view === "received" ? "secondary" : "outline"}
            role="tab"
            aria-selected={manage_view === "received"}
            onclick={() => manage_view = "received"}
            >Shared with me ({received_boards.length})</Button
          >
        </ButtonGroup.Root>
        <ScrollArea
          class="min-h-0 flex-1"
          orientation="vertical"
          scrollbarYClasses="w-2"
        >
          <div class="grid gap-3 pr-3 pb-1">
            {#if manage_view === "mine" && host_error}
              <p
                class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
                role="alert"
              >
                Sharing service could not start: {host_error}
              </p>
            {/if}
            {#if manage_view === "received"}
              {#if received_boards.length === 0}
                <Card.Root class="p-4 shadow-none"
                  ><Card.Description
                    >No boards have been shared with you.</Card.Description
                  ></Card.Root
                >
              {:else}
                {#each received_boards as shared}
                  <Card.Root class="gap-3 p-4 shadow-none">
                    <div
                      class="flex flex-wrap items-center justify-between gap-2"
                    >
                      <div>
                        <Card.Title class="text-base">{shared.name}</Card.Title
                        ><Card.Description
                          >{shared.sync_status === "syncing" ? "Syncing with owner..." : shared.sync_status === "conflict" ? "Sync conflict: both versions are saved" : shared.sync_status === "pending" ? "Local changes are waiting to sync" : shared.sync_status === "error" ? "Sync needs attention" : "Background sync enabled"}</Card.Description
                        >
                        <p class="mt-1 text-xs text-muted-foreground">
                          {board.iroh_last_synced_at[shared.id] ? `Last synced this session: ${sync_time.format(new Date(board.iroh_last_synced_at[shared.id]))}` : "No sync yet this session"}
                        </p>
                      </div>
                      <Badge variant="secondary"
                        >{shared.shared_role === "viewer" ? "Read only" : "Can edit"}</Badge
                      >
                    </div>
                    {#if shared.sync_status === "conflict"}
                      <p class="text-xs text-muted-foreground">
                        Open the board to review your local version. Using the
                        owner's version saves your local version as a separate
                        board first. Keeping yours will replace the owner's
                        changes if their version has not changed again.
                      </p>
                    {/if}
                    <div class="flex flex-wrap gap-2">
                      <Button
                        variant="outline"
                        onclick={() => { void board.switch_board(shared.id); open = false; }}
                        >Open board</Button
                      >
                      {#if shared.sync_status === "conflict"}
                        <Button
                          variant="outline"
                          onclick={() => void board.resolve_iroh_conflict(shared.id, false)}
                          >Use owner version</Button
                        >
                        {#if shared.shared_role === "editor"}
                          <Button
                            variant="outline"
                            onclick={() => void board.resolve_iroh_conflict(shared.id, true)}
                            >Keep my version</Button
                          >
                        {/if}
                      {:else}
                        <Button
                          variant="outline"
                          disabled={shared.sync_status === "syncing"}
                          onclick={() => void board.sync_iroh_board(shared.id)}
                          >{#if shared.sync_status === "syncing"}
                            <LoaderCircleIcon class="animate-spin" />
                            Syncing...
                          {:else}
                            Sync now
                          {/if}</Button
                        >
                      {/if}
                    </div>
                    {#if received_pending[shared.id]}
                      <Button
                        variant="secondary"
                        class="w-full"
                        onclick={() => received_waiting_id = shared.id}
                        >Waiting for approval · View device ID</Button
                      >
                    {:else if board.iroh_access_removed[shared.id]}
                      <div
                        class="grid gap-2 rounded-lg border border-primary/25 bg-primary/5 p-3 text-sm"
                      >
                        <span
                          >Access was removed. Your local board is still
                          here.</span
                        ><Button
                          class="w-full"
                          onclick={() => void request_received_access(shared.id, true)}
                          >Request access again</Button
                        >
                      </div>
                    {/if}
                  </Card.Root>
                {/each}
              {/if}
            {:else if invites.length === 0}
              <Card.Root class="p-4 shadow-none"
                ><Card.Description
                  >No invitations yet.</Card.Description
                ></Card.Root
              >
            {:else}
              {#if pending_count > 0}
                <div
                  class="flex items-center gap-3 rounded-lg border border-primary/25 bg-primary/5 p-3 text-sm"
                >
                  <ShieldCheckIcon class="size-5 text-primary" />
                  <span class="flex-1"
                    >{pending_count}
                    {pending_count === 1 ? "device is" : "devices are"}
                    waiting for your approval</span
                  >
                </div>
              {/if}
              {#each invites as invite}
                <Card.Root class="gap-3 p-4 shadow-none">
                  <div
                    class="flex flex-wrap items-center justify-between gap-2"
                  >
                    <div>
                      <Card.Title class="text-base"
                        >{invite.board_name}</Card.Title
                      ><Card.Description
                        >{invite.created_at || "Created before this version"}</Card.Description
                      >
                    </div>
                    <Badge variant={invite.enabled ? "secondary" : "outline"}
                      >{invite.enabled ? "Active" : "Disabled"}</Badge
                    >
                  </div>
                  <div class="flex flex-wrap items-center gap-2">
                    <Select.Root
                      type="single"
                      value={invite.permission}
                      onValueChange={(v) => void update(invite, { permission: v as "viewer" | "editor" })}
                    >
                      <Select.Trigger class="w-36"
                        >{invite.permission === "viewer" ? "Read only" : "Can edit"}</Select.Trigger
                      >
                      <Select.Content
                        ><Select.Item value="viewer">Read only</Select.Item
                        ><Select.Item value="editor"
                          >Can edit</Select.Item
                        ></Select.Content
                      >
                    </Select.Root>
                    <div class="flex items-center gap-2 px-1">
                      <Switch
                        id={`invite-active-${invite.invite_id}`}
                        checked={invite.enabled}
                        onCheckedChange={(checked) => void update(invite, { enabled: checked })}
                      />
                      <Label for={`invite-active-${invite.invite_id}`}
                        >Link active</Label
                      >
                    </div>
                  </div>
                  <div class="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      disabled={!invite.enabled || loading_access_id === invite.invite_id}
                      onclick={() => void copy_invite(invite)}
                      >Copy link</Button
                    >
                    <Button
                      variant="outline"
                      disabled={!invite.enabled || loading_access_id === invite.invite_id}
                      onclick={() => void toggle_qr(invite)}
                      >{visible_qr_id === invite.invite_id ? "Hide QR" : "Show QR"}</Button
                    >
                    <Button
                      variant="destructive"
                      onclick={() => { pending_delete = invite; delete_confirm_open = true; }}
                      >Delete</Button
                    >
                  </div>
                  {#if invite.devices.length > 0}
                    <Collapsible.Root
                      open={expanded_devices[invite.invite_id] ?? invite.devices.some((device) => device.status === IrohDeviceStatus.Pending)}
                      onOpenChange={(value) => expanded_devices = { ...expanded_devices, [invite.invite_id]: value }}
                      class="rounded-lg border bg-background"
                    >
                      <Collapsible.Trigger
                        class="group flex w-full items-center gap-2 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      >
                        <ShieldCheckIcon class="size-4 text-muted-foreground" />
                        <span class="flex-1"
                          >Devices
                          <span class="text-muted-foreground"
                            >({invite.devices.length})</span
                          ></span
                        >
                        {#if invite.devices.some((device) => device.status === IrohDeviceStatus.Pending)}
                          <Badge variant="secondary"
                            >{invite.devices.filter((device) => device.status === IrohDeviceStatus.Pending).length}
                            pending</Badge
                          >
                        {/if}
                        <ChevronDownIcon
                          class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
                        />
                      </Collapsible.Trigger>
                      <Collapsible.Content class="border-t">
                        <div class="max-h-56 space-y-2 overflow-y-auto p-2">
                          {#each invite.devices as device}
                            <div
                              class="rounded-lg border bg-muted/20 p-3 text-sm"
                            >
                              <div class="flex items-center gap-3">
                                <span
                                  class="flex size-9 shrink-0 items-center justify-center rounded-full bg-background text-primary"
                                  ><ShieldCheckIcon class="size-4" /></span
                                >
                                <div class="min-w-0 flex-1">
                                  <div class="font-medium">
                                    {device.status === IrohDeviceStatus.Pending ? "Approval requested" : "Approved device"}
                                  </div>
                                  <div
                                    class="font-mono text-xs text-muted-foreground"
                                  >
                                    {device.node_id.slice(0, 12)}…{device.node_id.slice(-8)}
                                  </div>
                                </div>
                                {#if device.status === IrohDeviceStatus.Pending}
                                  <Button
                                    size="sm"
                                    onclick={() => { approval_candidate = { invite, node_id: device.node_id }; approval_open = true; }}
                                    >Review</Button
                                  >
                                {:else}
                                  <Button
                                    variant="outline"
                                    size="sm"
                                    onclick={() => { revoke_candidate = { invite, node_id: device.node_id }; revoke_open = true; }}
                                    >Revoke</Button
                                  >
                                {/if}
                              </div>
                            </div>
                          {/each}
                        </div>
                      </Collapsible.Content>
                    </Collapsible.Root>
                  {/if}
                  {#if visible_qr_id === invite.invite_id && invite_access[invite.invite_id]}
                    <div
                      class="qr mx-auto aspect-square w-full max-w-56 overflow-hidden rounded-md bg-white p-2"
                      aria-label="Board invitation QR code"
                    >
                      {@html invite_access[invite.invite_id].qr_svg}
                    </div>
                  {/if}
                </Card.Root>
              {/each}
            {/if}
          </div>
        </ScrollArea>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
<AlertDialog.Root bind:open={delete_confirm_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title>Delete invitation?</AlertDialog.Title
      ><AlertDialog.Description
        >People can no longer use this invitation to sync. Boards they already
        received stay on their devices.</AlertDialog.Description
      ></AlertDialog.Header
    ><AlertDialog.Footer
      ><AlertDialog.Cancel
        onclick={() => { pending_delete = null; delete_confirm_open = false; }}
        >Cancel</AlertDialog.Cancel
      ><AlertDialog.Action onclick={() => void remove()}
        >Delete invitation</AlertDialog.Action
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
<AlertDialog.Root bind:open={approval_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title>Review device request</AlertDialog.Title
      ><AlertDialog.Description
        >Confirm this ID with the person requesting access through a trusted
        channel. Approval grants this invitation's
        {approval_candidate?.invite.permission === "editor" ? "editing" : "viewing"}
        permission. Rejected devices may request again
        later.</AlertDialog.Description
      ></AlertDialog.Header
    >
    {#if approval_candidate}
      <div class="rounded-lg border bg-muted/30 p-3">
        <div class="mb-2 text-xs font-medium text-muted-foreground">
          Device ID
        </div>
        <code class="block break-all text-sm leading-relaxed select-all"
          >{approval_candidate.node_id}</code
        ><Button
          variant="outline"
          size="sm"
          class="mt-3"
          onclick={() => void copy_device_id(approval_candidate!.node_id)}
          ><ClipboardIcon />
          Copy ID</Button
        >
      </div>
    {/if}
    <AlertDialog.Footer
      ><AlertDialog.Cancel>Back</AlertDialog.Cancel
      ><Button
        variant="destructive"
        onclick={() => { if (approval_candidate) void approve_device(approval_candidate.invite, approval_candidate.node_id, false); }}
        >Reject</Button
      ><AlertDialog.Action
        onclick={() => { if (approval_candidate) void approve_device(approval_candidate.invite, approval_candidate.node_id, true); }}
        >Approve device</AlertDialog.Action
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
<AlertDialog.Root bind:open={revoke_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title>Revoke device access?</AlertDialog.Title
      ><AlertDialog.Description
        >This device will no longer be able to sync
        {revoke_candidate?.invite.board_name ?? "this board"}. Its local copy
        stays on the device, and it can request access again
        later.</AlertDialog.Description
      ></AlertDialog.Header
    >
    {#if revoke_candidate}
      <code
        class="block break-all rounded-md border bg-muted/30 p-3 text-sm select-all"
        >{revoke_candidate.node_id}</code
      >
    {/if}
    <AlertDialog.Footer
      ><AlertDialog.Cancel>Cancel</AlertDialog.Cancel
      ><Button variant="destructive" onclick={() => void revoke_device()}
        >Revoke access</Button
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
<style>
.qr :global(svg) {
  display: block;
  width: 100%;
  height: 100%;
  max-width: 100%;
  max-height: 100%;
}
</style>
