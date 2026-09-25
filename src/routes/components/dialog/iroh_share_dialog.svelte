<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { toast } from "svelte-sonner";
  import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
  import LinkIcon from "@lucide/svelte/icons/link";
  import UsersIcon from "@lucide/svelte/icons/users";
  import Settings2Icon from "@lucide/svelte/icons/settings-2";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as ButtonGroup from "$lib/components/ui/button-group/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import type { BoardStore } from "../../board.svelte";

  type Flow = "home" | "share" | "join" | "manage";
  type Invite = { invite_id: string; board_id: number; board_name: string; permission: "viewer" | "editor"; enabled: boolean; created_at: string };
  type Generated = { ticket: string; qr: string; board_name: string; permission: "viewer" | "editor" };
  type InviteAccess = { ticket: string; qr_svg: string };
  let { open = $bindable(), board }: { open: boolean; board: BoardStore } = $props();
  let flow = $state<Flow>("home"), ticket = $state(""), permission = $state<"viewer" | "editor">("viewer"), selected_board_id = $state<number | null>(null);
  let generated = $state<Generated | null>(null), invites = $state<Invite[]>([]), joining = $state(false), creating = $state(false), pending_delete = $state<Invite | null>(null), delete_confirm_open = $state(false);
  let visible_qr_id = $state<string | null>(null), invite_access = $state<Record<string, InviteAccess>>({}), loading_access_id = $state<string | null>(null);
  let manage_view = $state<"mine" | "received">("mine");
  let host_error = $state<string | null>(null);
  const active = $derived(board.boards.find((item) => item.id === board.active_board_id));
  const owned_boards = $derived(board.boards.filter((item) => item.shared_role === "owner"));
  const selected_board = $derived(owned_boards.find((item) => item.id === selected_board_id));
  const received_boards = $derived(board.boards.filter((item) => item.shared_role !== "owner"));
  $effect(() => { if (selected_board_id === null) selected_board_id = active?.shared_role === "owner" ? active.id : (owned_boards[0]?.id ?? null); });
  $effect(() => {
    if (open) return;
    flow = "home";
    ticket = "";
    board.iroh_last_error = "";
    permission = "viewer";
    generated = null;
    manage_view = "mine";
    visible_qr_id = null;
  });
  function invalidate() { generated = null; }
  function open_join() { ticket = ""; board.iroh_last_error = ""; flow = "join"; }
  async function create() { if (selected_board_id === null) return; creating = true; try { const created = await board.create_iroh_invite(selected_board_id, permission); generated = { ticket: created, qr: await invoke<string>("iroh_invite_qr_svg", { ticket: created }), board_name: selected_board?.name ?? "Board", permission }; let copied = false; try { await navigator.clipboard.writeText(created); copied = true; } catch { /* The visible link can still be copied later. */ } await load(); toast.success(copied ? "Invitation copied" : "Invitation created"); } catch (e) { toast.error(e instanceof Error ? e.message : "Couldn't create invitation"); } finally { creating = false; } }
  async function copy() { if (!generated) return; try { await navigator.clipboard.writeText(generated.ticket); toast.success("Invitation copied"); } catch { toast.error("Couldn't copy invitation"); } }
  async function join() { joining = true; board.iroh_last_error = ""; try { if (await board.join_iroh_invite(ticket)) open = false; } finally { joining = false; } }
  async function load() { try { invites = await invoke<Invite[]>("list_iroh_invites"); host_error = await invoke<string | null>("iroh_host_error"); } catch (e) { toast.error(e instanceof Error ? e.message : "Couldn't load invitations"); } }
  async function update(invite: Invite, change: { permission?: "viewer" | "editor"; enabled?: boolean }) { try { await invoke("update_iroh_invite", { inviteId: invite.invite_id, ...change }); visible_qr_id = null; const next = { ...invite_access }; delete next[invite.invite_id]; invite_access = next; await load(); if (change.enabled === true && host_error) toast.error("Invitation saved, but sharing is offline"); else toast.success("Invitation updated"); } catch (e) { await load(); toast.error(e instanceof Error ? e.message : "Couldn't update invitation"); } }
  async function remove() { if (!pending_delete) return; try { await invoke("delete_iroh_invite", { inviteId: pending_delete.invite_id }); await load(); toast.success("Invitation deleted"); } catch (e) { toast.error(e instanceof Error ? e.message : "Couldn't delete invitation"); } finally { pending_delete = null; delete_confirm_open = false; } }
  async function access(invite: Invite): Promise<InviteAccess | null> { if (invite_access[invite.invite_id]) return invite_access[invite.invite_id]; loading_access_id = invite.invite_id; try { const value = await invoke<InviteAccess>("get_iroh_invite_access", { inviteId: invite.invite_id }); invite_access = { ...invite_access, [invite.invite_id]: value }; return value; } catch (e) { toast.error(e instanceof Error ? e.message : "Couldn't load invitation"); return null; } finally { loading_access_id = null; } }
  async function copy_invite(invite: Invite) { const value = await access(invite); if (!value) return; try { await navigator.clipboard.writeText(value.ticket); toast.success("Invitation copied"); } catch { toast.error("Couldn't copy invitation"); } }
  async function toggle_qr(invite: Invite) { if (visible_qr_id === invite.invite_id) { visible_qr_id = null; return; } if (await access(invite)) visible_qr_id = invite.invite_id; }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class={flow === "manage"
    ? "flex h-[min(42rem,calc(100dvh-2rem))] w-[calc(100vw-2rem)] flex-col overflow-hidden sm:max-w-xl"
    : "max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] overflow-y-auto sm:max-w-xl"}>
    <Dialog.Header>
      <Dialog.Title>{flow === "home" ? "Board sharing" : flow === "share" ? "Share a board" : flow === "join" ? "Join a shared board" : "Manage board sharing"}</Dialog.Title>
      {#if flow === "home"}
        <Dialog.Description>Collaborate in Cardbe with read-only or editing access.</Dialog.Description>
      {:else if flow === "manage"}
        <Dialog.Description>Manage boards you share and boards shared with you.</Dialog.Description>
      {/if}
    </Dialog.Header>
    <div class={flow === "manage"
      ? "flex min-h-0 flex-1 flex-col gap-4"
      : flow === "home"
        ? "grid gap-2"
        : "grid gap-4"}>
      {#if flow !== "home"}<Button variant="ghost" class="w-fit" onclick={() => flow = "home"}><ArrowLeftIcon /> Back</Button>{/if}
      {#if flow === "home"}
        <div class="grid gap-3 sm:grid-cols-2"><button class="rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" onclick={() => flow = "share"}><Card.Root class="h-full gap-2 p-4 transition-colors hover:bg-accent"><UsersIcon class="size-5 text-primary"/><Card.Title class="text-base">Share a board</Card.Title><Card.Description>Choose a board and give people viewing or editing access.</Card.Description></Card.Root></button><button class="rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" onclick={open_join}><Card.Root class="h-full gap-2 p-4 transition-colors hover:bg-accent"><LinkIcon class="size-5 text-primary"/><Card.Title class="text-base">Join a shared board</Card.Title><Card.Description>Paste an invitation link to join.</Card.Description></Card.Root></button></div>
        <Button variant="outline" onclick={() => { void load(); flow = "manage"; }}><Settings2Icon /> Manage sharing</Button>
      {:else if flow === "share"}
        <div class="grid gap-4"><label class="grid gap-2 text-sm font-medium">Board<Select.Root type="single" value={selected_board_id?.toString() ?? ""} onValueChange={(v) => { selected_board_id = v ? Number(v) : null; invalidate(); }}><Select.Trigger class="w-full">{selected_board?.name ?? "Choose a board"}</Select.Trigger><Select.Content>{#each owned_boards as item}<Select.Item value={item.id.toString()}>{item.name}</Select.Item>{/each}</Select.Content></Select.Root></label><label class="grid gap-2 text-sm font-medium">Permission<Select.Root type="single" value={permission} onValueChange={(v) => { permission = v as "viewer" | "editor"; invalidate(); }}><Select.Trigger class="w-full">{permission === "viewer" ? "Read only" : "Can edit"}</Select.Trigger><Select.Content><Select.Item value="viewer">Read only</Select.Item><Select.Item value="editor">Can edit</Select.Item></Select.Content></Select.Root></label>
        {#if generated}<Card.Root class="gap-3 bg-muted/30 p-4 shadow-none"><div class="flex items-center justify-between gap-2"><Card.Title class="text-base">{generated.board_name}</Card.Title><Badge variant="secondary">{generated.permission === "viewer" ? "Read only" : "Can edit"}</Badge></div><div class="qr mx-auto aspect-square w-full max-w-56 overflow-hidden rounded-md bg-white p-2">{@html generated.qr}</div><Button variant="outline" onclick={() => void copy()}>Copy invitation link</Button><Card.Description>Anyone with this link receives this permission. Create another invitation to manage access separately.</Card.Description></Card.Root>{/if}<Button onclick={() => void create()} disabled={creating || selected_board_id === null}>{creating ? "Creating..." : generated ? "Create new invitation" : "Create invitation"}</Button></div>
      {:else if flow === "join"}
        <label class="grid gap-2 text-sm font-medium">Invitation link<Input bind:value={ticket} placeholder="Paste invitation link" oninput={() => board.iroh_last_error = ""} /></label><p class="text-xs text-muted-foreground">The board appears separately in your board list.</p>{#if board.iroh_last_error}<p class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive" role="alert">{board.iroh_last_error}</p>{/if}<Button onclick={() => void join()} disabled={!ticket.trim() || joining}>{joining ? "Connecting..." : "Join board"}</Button>
      {:else}
        <ButtonGroup.Root class="grid grid-cols-2" role="tablist" aria-label="Sharing management views">
          <Button variant={manage_view === "mine" ? "secondary" : "outline"} role="tab" aria-selected={manage_view === "mine"} onclick={() => manage_view = "mine"}>Shared by me ({invites.length})</Button>
          <Button variant={manage_view === "received" ? "secondary" : "outline"} role="tab" aria-selected={manage_view === "received"} onclick={() => manage_view = "received"}>Shared with me ({received_boards.length})</Button>
        </ButtonGroup.Root>
        <ScrollArea class="min-h-0 flex-1" orientation="vertical" scrollbarYClasses="w-2">
          <div class="grid gap-3 pr-3 pb-1">
          {#if manage_view === "mine" && host_error}<p class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive" role="alert">Sharing service could not start: {host_error}</p>{/if}
          {#if manage_view === "received"}
            {#if received_boards.length === 0}
              <Card.Root class="p-4 shadow-none"><Card.Description>No boards have been shared with you.</Card.Description></Card.Root>
            {:else}
              {#each received_boards as shared}
                <Card.Root class="gap-3 p-4 shadow-none">
                  <div class="flex flex-wrap items-center justify-between gap-2"><div><Card.Title class="text-base">{shared.name}</Card.Title><Card.Description>{shared.sync_status === "conflict" ? "Sync conflict: both versions are saved" : shared.sync_status === "pending" ? "Local changes are waiting to sync" : shared.sync_status === "error" ? "Sync needs attention" : "Background sync enabled"}</Card.Description></div><Badge variant="secondary">{shared.shared_role === "viewer" ? "Read only" : "Can edit"}</Badge></div>
                  {#if shared.sync_status === "conflict"}<p class="text-xs text-muted-foreground">Open the board to review your local version. Using the owner's version saves your local version as a separate board first. Keeping yours will replace the owner's changes if their version has not changed again.</p>{/if}
                  <div class="flex flex-wrap gap-2"><Button variant="outline" onclick={() => { void board.switch_board(shared.id); open = false; }}>Open board</Button>{#if shared.sync_status === "conflict"}<Button variant="outline" onclick={() => void board.resolve_iroh_conflict(shared.id, false)}>Use owner version</Button>{#if shared.shared_role === "editor"}<Button variant="outline" onclick={() => void board.resolve_iroh_conflict(shared.id, true)}>Keep my version</Button>{/if}{:else}<Button variant="outline" onclick={() => void board.sync_iroh_board(shared.id)}>Sync now</Button>{/if}</div>
                </Card.Root>
              {/each}
            {/if}
          {:else if invites.length === 0}
            <Card.Root class="p-4 shadow-none"><Card.Description>No invitations yet.</Card.Description></Card.Root>
          {:else}
            {#each invites as invite}
              <Card.Root class="gap-3 p-4 shadow-none">
                <div class="flex flex-wrap items-center justify-between gap-2">
                  <div><Card.Title class="text-base">{invite.board_name}</Card.Title><Card.Description>{invite.created_at || "Created before this version"}</Card.Description></div>
                  <Badge variant={invite.enabled ? "secondary" : "outline"}>{invite.enabled ? "Active" : "Disabled"}</Badge>
                </div>
                <div class="flex flex-wrap items-center gap-2">
                  <Select.Root type="single" value={invite.permission} onValueChange={(v) => void update(invite, { permission: v as "viewer" | "editor" })}>
                    <Select.Trigger class="w-36">{invite.permission === "viewer" ? "Read only" : "Can edit"}</Select.Trigger>
                    <Select.Content><Select.Item value="viewer">Read only</Select.Item><Select.Item value="editor">Can edit</Select.Item></Select.Content>
                  </Select.Root>
                  <div class="flex items-center gap-2 px-1">
                    <Switch id={`invite-active-${invite.invite_id}`} checked={invite.enabled} onCheckedChange={(checked) => void update(invite, { enabled: checked })} />
                    <Label for={`invite-active-${invite.invite_id}`}>Link active</Label>
                  </div>
                </div>
                <div class="flex flex-wrap gap-2">
                  <Button variant="outline" disabled={!invite.enabled || loading_access_id === invite.invite_id} onclick={() => void copy_invite(invite)}>Copy link</Button>
                  <Button variant="outline" disabled={!invite.enabled || loading_access_id === invite.invite_id} onclick={() => void toggle_qr(invite)}>{visible_qr_id === invite.invite_id ? "Hide QR" : "Show QR"}</Button>
                  <Button variant="destructive" onclick={() => { pending_delete = invite; delete_confirm_open = true; }}>Delete</Button>
                </div>
                {#if visible_qr_id === invite.invite_id && invite_access[invite.invite_id]}
                  <div class="qr mx-auto aspect-square w-full max-w-56 overflow-hidden rounded-md bg-white p-2" aria-label="Board invitation QR code">{@html invite_access[invite.invite_id].qr_svg}</div>
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
<AlertDialog.Root bind:open={delete_confirm_open}><AlertDialog.Content><AlertDialog.Header><AlertDialog.Title>Delete invitation?</AlertDialog.Title><AlertDialog.Description>People can no longer use this invitation to sync. Boards they already received stay on their devices.</AlertDialog.Description></AlertDialog.Header><AlertDialog.Footer><AlertDialog.Cancel onclick={() => { pending_delete = null; delete_confirm_open = false; }}>Cancel</AlertDialog.Cancel><AlertDialog.Action onclick={() => void remove()}>Delete invitation</AlertDialog.Action></AlertDialog.Footer></AlertDialog.Content></AlertDialog.Root>
<style>.qr :global(svg) { display:block; width:100%; height:100%; max-width:100%; max-height:100%; }</style>
