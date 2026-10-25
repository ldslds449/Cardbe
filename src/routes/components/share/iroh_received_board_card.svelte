<script lang="ts">
  import {
    translateCommandError,
    type CommandError,
  } from "#lib/command-errors.js";
  import { getLocale } from "#lib/i18n/index.js";
  import * as m from "#lib/paraglide/messages.js";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { Label } from "#lib/components/ui/label/index.js";
  import type { BoardStore, BoardSummary } from "../../board.svelte";
  import type { IrohConnectionInfo as ConnectionInfo } from "../../type/iroh-share";
  import IrohConnectionInfo from "./iroh_connection_info.svelte";

  let {
    board,
    shared,
    pending_device_id,
    connection,
    connection_dialog_open,
    connection_ticket,
    connection_error,
    saving_connection,
    saving_this_connection,
    onopen,
    onshowwaiting,
    onrequestaccess,
    onconnectiondialogchange,
    onticketinput,
    onsaveconnection,
  }: {
    board: BoardStore;
    shared: BoardSummary;
    pending_device_id: string | undefined;
    connection: ConnectionInfo | undefined;
    connection_dialog_open: boolean;
    connection_ticket: string;
    connection_error: CommandError | undefined;
    saving_connection: boolean;
    saving_this_connection: boolean;
    onopen: () => void;
    onshowwaiting: () => void;
    onrequestaccess: () => void;
    onconnectiondialogchange: (open: boolean) => void;
    onticketinput: (value: string) => void;
    onsaveconnection: () => void;
  } = $props();
  const sync_time = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      dateStyle: "medium",
      timeStyle: "short",
    }),
  );
</script>

<Card.Root class="gap-3 p-4 shadow-none">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div>
      <Card.Title class="text-base">{shared.name}</Card.Title><Card.Description
        >{board.iroh_access_removed[shared.id]
          ? board.iroh_access_error[shared.id]
            ? translateCommandError(board.iroh_access_error[shared.id])
            : m.share_access_unavailable()
          : shared.sync_status === "syncing"
            ? m.ui_syncing_with_owner()
            : shared.sync_status === "conflict"
              ? m.ui_sync_conflict_both_versions_are_saved()
              : shared.sync_status === "pending"
                ? m.ui_local_changes_are_waiting_to_sync()
                : shared.sync_status === "error"
                  ? m.ui_sync_needs_attention()
                  : m.ui_background_sync_enabled()}</Card.Description
      >
      <p class="mt-1 text-xs text-muted-foreground">
        {board.iroh_last_synced_at[shared.id]
          ? m.share_last_synced({
              time: sync_time.format(
                new Date(board.iroh_last_synced_at[shared.id]),
              ),
            })
          : m.ui_no_sync_yet_this_session()}
      </p>
    </div>
    <Badge variant="secondary"
      >{shared.shared_role === "viewer"
        ? m.board_read_only()
        : m.board_can_edit()}</Badge
    >
  </div>
  {#if shared.sync_status === "conflict"}
    <p class="text-xs text-muted-foreground">
      {m.share_conflict_description()}
    </p>
  {/if}
  <div class="flex flex-wrap gap-2">
    <Button variant="outline" onclick={onopen}>{m.ui_open_board()}</Button>
    {#if shared.sync_status === "conflict"}
      <Button
        variant="outline"
        onclick={() => void board.resolve_iroh_conflict(shared.id, false)}
        >{m.ui_use_owner_version()}</Button
      >
      {#if shared.shared_role === "editor"}
        <Button
          variant="outline"
          onclick={() => void board.resolve_iroh_conflict(shared.id, true)}
          >{m.ui_keep_my_version()}</Button
        >
      {/if}
    {:else}
      <Button
        variant="outline"
        disabled={shared.sync_status === "syncing" ||
          !!board.iroh_access_removed[shared.id] ||
          !!pending_device_id}
        onclick={() => void board.sync_iroh_board(shared.id)}
        >{#if shared.sync_status === "syncing"}
          <LoaderCircleIcon class="animate-spin" />
          {m.share_syncing()}
        {:else}
          {m.share_sync()}
        {/if}</Button
      >
    {/if}

    <Dialog.Root
      open={connection_dialog_open}
      onOpenChange={onconnectiondialogchange}
    >
      <Dialog.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="outline" disabled={saving_connection}>
            {m.share_update_connection()}
          </Button>
        {/snippet}
      </Dialog.Trigger>
      <Dialog.Content class="sm:max-w-md">
        <form
          class="grid gap-4"
          onsubmit={(event) => {
            event.preventDefault();
            onsaveconnection();
          }}
        >
          <Dialog.Header>
            <Dialog.Title>{m.share_update_connection()}</Dialog.Title>
            <Dialog.Description id={`connection-help-${shared.id}`}>
              <span class="mb-2 block break-words font-medium text-foreground"
                >{shared.name}</span
              >
              {m.share_update_connection_help()}
            </Dialog.Description>
          </Dialog.Header>
          <div class="grid gap-2">
            <Label for={`connection-ticket-${shared.id}`} class="text-xs">
              {m.ui_invitation_link()}
            </Label>
            <Input
              class="min-w-0"
              placeholder="cardbe://share/..."
              id={`connection-ticket-${shared.id}`}
              value={connection_ticket}
              oninput={(event) => onticketinput(event.currentTarget.value)}
              disabled={saving_connection}
              aria-describedby={`connection-help-${shared.id}${connection_error ? ` connection-error-${shared.id}` : ""}`}
              aria-invalid={!!connection_error}
              autocomplete="off"
              spellcheck={false}
            />

            {#if connection_error}
              <p
                id={`connection-error-${shared.id}`}
                role="alert"
                class="text-sm text-destructive"
              >
                {translateCommandError(connection_error)}
              </p>
            {/if}
          </div>
          <Dialog.Footer>
            <Dialog.Close>
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="outline"
                  disabled={saving_connection}>{m.common_cancel()}</Button
                >
              {/snippet}
            </Dialog.Close>
            <Button
              type="submit"
              class="shrink-0"
              disabled={saving_connection || !connection_ticket.trim()}
            >
              {#if saving_this_connection}<LoaderCircleIcon
                  class="animate-spin"
                />{/if}
              {saving_this_connection ? m.ui_saving() : m.ui_save_changes()}
            </Button>
          </Dialog.Footer>
        </form>
      </Dialog.Content>
    </Dialog.Root>
  </div>
  {#if pending_device_id}
    <Button variant="secondary" class="w-full" onclick={onshowwaiting}
      >{m.ui_waiting_for_approval_view_device_id()}</Button
    >
  {:else if board.iroh_access_removed[shared.id]}
    <div
      class="grid gap-2 rounded-lg border border-primary/25 bg-primary/5 p-3 text-sm"
    >
      <span
        >{board.iroh_access_error[shared.id]
          ? translateCommandError(board.iroh_access_error[shared.id])
          : m.share_access_unavailable()}
        {m.share_access_removed_help()}</span
      ><Button class="w-full" onclick={onrequestaccess}
        >{m.ui_request_access_again()}</Button
      >
    </div>
  {/if}
  <IrohConnectionInfo info={connection} />
  {#if board.iroh_sync_error[shared.id] && !board.iroh_access_removed[shared.id]}
    <p class="text-sm text-destructive" role="status">
      {translateCommandError(board.iroh_sync_error[shared.id])}
    </p>
  {/if}
</Card.Root>
