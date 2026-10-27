<script lang="ts">
  import { formatNumber, getLocale } from "#lib/i18n/index.js";
  import * as m from "#lib/paraglide/messages.js";
  import {
    type CommandError,
    parseCommandError,
    translateCommandError,
  } from "#lib/command-errors.js";
  import { logger } from "#lib/logger.js";
  import { invoke } from "@tauri-apps/api/core";
  import { toast } from "svelte-sonner";
  import ArrowLeftIcon from "@lucide/svelte/icons/arrow-left";
  import LinkIcon from "@lucide/svelte/icons/link";
  import UsersIcon from "@lucide/svelte/icons/users";
  import Settings2Icon from "@lucide/svelte/icons/settings-2";
  import ClipboardIcon from "@lucide/svelte/icons/clipboard";
  import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import { onMount, tick, untrack } from "svelte";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as ButtonGroup from "#lib/components/ui/button-group/index.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as AlertDialog from "#lib/components/ui/alert-dialog/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { Label } from "#lib/components/ui/label/index.js";
  import * as Select from "#lib/components/ui/select/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Checkbox } from "#lib/components/ui/checkbox/index.js";
  import type { BoardStore, BoardSummary } from "../../board.svelte";
  import IrohConnectionInfo from "../share/iroh_connection_info.svelte";
  import IrohInviteCard from "../share/iroh_invite_card.svelte";
  import {
    openSettings,
    settingsDialog,
  } from "../workspace/settings-dialog.svelte";
  import IrohReceivedBoardCard from "../share/iroh_received_board_card.svelte";
  import {
    IrohDeviceStatus,
    type IrohInvite,
    type IrohConnectionDetails,
  } from "../../type/iroh-share";

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
    initial_ticket = $bindable(""),
    show_requests = false,
    show_received = false,
    onRequestsChanged = () => {},
  }: {
    open: boolean;
    board: BoardStore;
    initial_ticket?: string;
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
  let create_error = $state<CommandError | null>(null);
  let create_generation = 0;
  let qr_loading = $state(false);
  let qr_error = $state<CommandError | null>(null);
  let visible_qr_id = $state<string | null>(null),
    invite_access = $state<Record<string, InviteAccess>>({}),
    loading_access_id = $state<string | null>(null);
  let manage_view = $state<"mine" | "received">("mine");
  let manage_view_button = $state<HTMLButtonElement | null>(null);
  let host_error = $state<unknown>(null);
  let managed_board_id = $state<number | null>(null);
  let detail_heading = $state<HTMLHeadingElement | null>(null);
  let manage_viewport = $state<HTMLDivElement | null>(null);
  let share_return_to_manage = $state(false);
  let loading_invites = $state(false);
  let manage_error = $state<unknown>(null);
  let device_action_pending = $state(false);
  let device_id_verified = $state(false);
  let updating_invite_id = $state<string | null>(null);
  let connection_details = $state<IrohConnectionDetails>({
    invitation: null,
    devices: {},
    boards: {},
  });
  let connection_details_error = $state<unknown>(null);
  let connection_tickets = $state<Record<number, string>>({});
  let connection_update_errors = $state<Record<number, CommandError>>({});
  let saving_connection_id = $state<number | null>(null);
  let connection_dialog_id = $state<number | null>(null);
  function group_invites(items: Invite[]) {
    const groups = new Map<
      number,
      {
        board_id: number;
        board_name: string;
        invites: Invite[];
        approved: number;
      }
    >();
    for (const invite of items) {
      let group = groups.get(invite.board_id);
      if (!group) {
        group = {
          board_id: invite.board_id,
          board_name: invite.board_name,
          invites: [],
          approved: 0,
        };
        groups.set(invite.board_id, group);
      }
      group.invites.push(invite);
      group.approved += invite.devices.filter(
        (device) => device.status === IrohDeviceStatus.Approved,
      ).length;
    }
    return [...groups.values()];
  }
  const shared_boards = $derived(group_invites(invites));
  const managed_board = $derived(
    shared_boards.find((item) => item.board_id === managed_board_id),
  );
  const visible_invites = $derived(managed_board?.invites ?? []);
  const pending_requests = $derived(
    invites.flatMap((invite) =>
      invite.devices
        .filter((device) => device.status === IrohDeviceStatus.Pending)
        .map((device) => ({ invite, device })),
    ),
  );
  async function open_managed_board(id: number) {
    managed_board_id = id;
    visible_qr_id = null;
    await tick();
    manage_viewport?.scrollTo({ top: 0 });
    detail_heading?.focus({ preventScroll: true });
  }
  async function back_to_overview() {
    managed_board_id = null;
    visible_qr_id = null;
    await tick();
    manage_viewport?.scrollTo({ top: 0 });
    manage_view_button?.focus({ preventScroll: true });
  }
  function open_share() {
    share_return_to_manage = flow === "manage";
    if (share_return_to_manage) {
      selected_board_id = managed_board?.board_id ?? selected_board_id;
    }
    invalidate();
    flow = "share";
  }
  async function leave_share() {
    invalidate();
    flow = share_return_to_manage ? "manage" : "home";
    await tick();
    if (share_return_to_manage) {
      manage_viewport?.scrollTo({ top: 0 });
      (detail_heading ?? manage_view_button)?.focus({ preventScroll: true });
    }
  }
  const sync_time = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      dateStyle: "medium",
      timeStyle: "short",
    }),
  );
  function invitation_date(value: string) {
    // SQLite datetime('now') is UTC but omits the timezone.
    const date = new Date(
      /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(value)
        ? `${value.replace(" ", "T")}Z`
        : value,
    );
    return Number.isNaN(date.getTime())
      ? m.share_creation_date_unavailable()
      : m.share_created_date({ date: sync_time.format(date) });
  }

  function verification_id(id: string) {
    return id.match(/.{1,4}/g)?.join(" ") ?? id;
  }
  async function load_connection_details() {
    try {
      connection_details = await invoke<IrohConnectionDetails>(
        "get_iroh_connection_details",
        {
          boardIds: received_boards.map((shared) => shared.id),
          ticket: waiting_device_id ? ticket : null,
        },
      );
      connection_details_error = null;
    } catch (error) {
      logger.warn("iroh.connection_details.failed", error);
      connection_details_error = parseCommandError(error) ?? {
        code: "INTERNAL_ERROR",
      };
    }
  }
  // Invitations and access details belong to the previous network settings.
  let network_version = settingsDialog.networkVersion;
  $effect(() => {
    const version = settingsDialog.networkVersion;
    if (version === network_version) {
      return;
    }
    network_version = version;
    untrack(() => {
      invite_access = {};
      visible_qr_id = null;
      generated = null;
      if (open) {
        void load_connection_details();
      }
    });
  });
  let waiting_device_id = $state<string | null>(null);
  let checking_approval = false;
  let wait_generation = 0;
  let approval_candidate = $state<{ invite: Invite; node_id: string } | null>(
    null,
  );
  let approval_open = $state(false);
  let approval_trigger = $state<HTMLElement | null>(null);
  function open_device_review(
    invite: Invite,
    node_id: string,
    trigger: HTMLElement,
  ) {
    approval_trigger = trigger;
    approval_candidate = { invite, node_id };
    device_id_verified = false;
    approval_open = true;
  }
  async function restore_review_focus() {
    await tick();
    const target = approval_trigger?.isConnected
      ? approval_trigger
      : detail_heading?.isConnected
        ? detail_heading
        : manage_view_button;
    target?.focus({ preventScroll: true });
    approval_trigger = null;
  }
  let revoke_candidate = $state<{ invite: Invite; node_id: string } | null>(
    null,
  );
  let revoke_open = $state(false);
  let received_pending = $state<Record<number, string>>({});
  let received_waiting_id = $state<number | null>(null);
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
  $effect(() => {
    const current_ids = new Set(received_boards.map((shared) => shared.id));
    for (const id of Object.keys(received_pending).map(Number)) {
      if (!current_ids.has(id)) {
        stopped_waiting.add(id);
        delete received_pending[id];
      }
    }
    if (received_waiting_id !== null && !current_ids.has(received_waiting_id)) {
      stopped_waiting.add(received_waiting_id);
      received_waiting_id = null;
    }
  });
  onMount(() => {
    const timer = window.setInterval(() => {
      if (open && flow === "manage") {
        void load(true);
      }
      if (
        open &&
        flow === "join" &&
        waiting_device_id &&
        !joining &&
        !checking_approval
      ) {
        void check_approval();
      }
      if (open) {
        for (const id of Object.keys(received_pending)) {
          void request_received_access(Number(id), false);
        }
      }
    }, 5000);
    return () => {
      window.clearInterval(timer);
    };
  });
  $effect(() => {
    if (open && (show_requests || show_received)) {
      const view = show_received ? "received" : "mine";
      untrack(() => {
        flow = "manage";
        manage_view = view;
        managed_board_id = null;
        void load();
      });
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
    if (selected_board_id === null) {
      selected_board_id =
        active?.shared_role === "owner"
          ? active.id
          : (owned_boards[0]?.id ?? null);
    }
  });
  $effect(() => {
    if (open) {
      return;
    }
    flow = "home";
    ticket = "";
    board.iroh_last_error = null;
    permission = "viewer";
    invalidate();
    waiting_device_id = null;
    received_waiting_id = null;
    manage_view = "mine";
    managed_board_id = null;
    share_return_to_manage = false;
    approval_trigger = null;
    visible_qr_id = null;
    connection_dialog_id = null;
    connection_tickets = {};
    connection_update_errors = {};
  });
  async function update_connection(board_id: number) {
    if (saving_connection_id !== null) {
      return;
    }
    saving_connection_id = board_id;
    delete connection_update_errors[board_id];
    try {
      await invoke("update_iroh_connection", {
        boardId: board_id,
        ticket: connection_tickets[board_id]?.trim() ?? "",
      });
      delete connection_tickets[board_id];
      if (connection_dialog_id === board_id) {
        connection_dialog_id = null;
      }
      toast.success(m.share_connection_updated());
      await load_connection_details();
    } catch (error) {
      logger.warn("iroh.connection_update.failed", error);
      connection_update_errors[board_id] = parseCommandError(error) ?? {
        code: "INTERNAL_ERROR",
      };
    } finally {
      saving_connection_id = null;
    }
  }
  function invalidate() {
    create_generation++;
    generated = null;
    create_error = null;
    qr_error = null;
    qr_loading = false;
  }
  $effect(() => {
    if (open && initial_ticket) {
      const value = initial_ticket;
      untrack(() => {
        wait_generation++;
        received_waiting_id = null;
        approval_open = false;
        revoke_open = false;
        delete_confirm_open = false;
        open_join();
        ticket = value;
        initial_ticket = "";
      });
    }
  });
  function open_join() {
    ticket = "";
    waiting_device_id = null;
    board.iroh_last_error = null;
    flow = "join";
  }
  function open_network_settings() {
    open = false;
    openSettings("sharing");
  }
  async function create() {
    if (creating || selected_board_id === null) {
      return;
    }
    const board_id = selected_board_id;
    const board_name = selected_board?.name ?? m.board_title();
    const requested_permission = permission;
    invalidate();
    const generation = create_generation;
    const current = () =>
      generation === create_generation && open && flow === "share";
    creating = true;
    try {
      const created = await board.create_iroh_invite(
        board_id,
        requested_permission,
      );
      if (!current()) {
        return;
      }
      generated = {
        ticket: created,
        qr: "",
        board_name,
        permission: requested_permission,
      };
      let copied = false;
      try {
        await navigator.clipboard.writeText(created);
        copied = true;
      } catch (e) {
        logger.warn("iroh.invite_create_copy.failed", e);
        /* The visible link can still be copied later. */
      }
      if (!current()) {
        return;
      }
      await load(true);
      if (!current()) {
        return;
      }
      toast.success(
        copied ? m.ui_invitation_copied() : m.ui_invitation_created(),
      );
      void generate_qr();
    } catch (e) {
      logger.error("iroh.invite_create.failed", e);
      if (!current()) {
        return;
      }
      create_error = parseCommandError(e) ?? { code: "INTERNAL_ERROR" };
      toast.error(translateCommandError(e));
    } finally {
      creating = false;
    }
  }
  async function generate_qr() {
    const target = generated;
    if (!target || qr_loading) {
      return;
    }
    const generation = create_generation;
    const current = () =>
      generation === create_generation && open && flow === "share";
    qr_loading = true;
    qr_error = null;
    try {
      const qr = await invoke<string>("iroh_invite_qr_svg", {
        ticket: target.ticket,
      });
      if (current()) {
        generated = { ...target, qr };
      }
    } catch (error) {
      logger.warn("iroh.invite_qr.failed", error);
      if (current()) {
        qr_error = parseCommandError(error) ?? { code: "INTERNAL_ERROR" };
      }
    } finally {
      if (generation === create_generation) {
        qr_loading = false;
      }
    }
  }
  async function copy() {
    if (!generated) {
      return;
    }
    try {
      await navigator.clipboard.writeText(generated.ticket);
      toast.success(m.ui_invitation_copied());
    } catch (e) {
      logger.warn("iroh.invite_copy.failed", e);
      toast.error(m.ui_couldn_t_copy_invitation());
    }
  }
  async function join(silent = false) {
    const generation = wait_generation;
    joining = true;
    board.iroh_last_error = null;
    try {
      const joined = await board.join_iroh_invite(ticket, silent);
      if (generation !== wait_generation) {
        board.iroh_last_error = null;
        return;
      }
      if (joined) {
        open = false;
        return;
      }
      const last_error = parseCommandError(board.iroh_last_error);
      if (last_error?.code === "SHARE_APPROVAL_REQUIRED") {
        waiting_device_id = last_error.device_id;
        board.iroh_last_error = null;
        await load_connection_details();
      } else if (silent && last_error?.code === "SHARE_ACCESS_REVOKED") {
        waiting_device_id = null;
      }
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
    if (loading_invites) {
      return;
    }
    loading_invites = true;
    try {
      invites = await invoke<Invite[]>("list_iroh_invites");
      host_error = await invoke<unknown>("iroh_host_error");
      if (host_error) {
        logger.error("iroh.host.failed", host_error);
      }
      onRequestsChanged();
      manage_error = null;
      await load_connection_details();
    } catch (e) {
      logger.error("iroh.invite_list_load.failed", e);
      manage_error = parseCommandError(e) ?? { code: "INTERNAL_ERROR" };
      if (!silent) {
        toast.error(translateCommandError(e));
      }
    } finally {
      loading_invites = false;
    }
  }
  async function approve_device(
    invite: Invite,
    node_id: string,
    approved: boolean,
  ) {
    if (device_action_pending || (approved && !device_id_verified)) {
      return false;
    }
    device_action_pending = true;
    try {
      await invoke("set_iroh_device_approved", {
        inviteId: invite.invite_id,
        nodeId: node_id,
        approved,
      });
      await load();
      approval_open = false;
      toast.success(
        approved
          ? m.ui_device_approved()
          : invite.devices.find((device) => device.node_id === node_id)
                ?.status === IrohDeviceStatus.Pending
            ? m.ui_request_rejected()
            : m.ui_device_access_revoked(),
      );
      return true;
    } catch (e) {
      logger.error("iroh.device_approval.failed", e);
      toast.error(translateCommandError(e));
      return false;
    } finally {
      device_action_pending = false;
    }
  }
  async function revoke_device() {
    if (!revoke_candidate) {
      return;
    }
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
      if (request_approval) {
        received_retry_request.add(board_id);
      }
      return;
    }
    received_requesting.add(board_id);
    try {
      const [approved, device_id, refreshed] = await invoke<
        [boolean, string, BoardSummary?]
      >("request_iroh_board_access", {
        boardId: board_id,
        requestApproval: request_approval,
      });
      if (stopped_waiting.has(board_id)) {
        return;
      }
      if (refreshed) {
        board.apply_iroh_conflict_refresh(refreshed);
      }
      if (approved) {
        delete received_pending[board_id];
        received_waiting_id = null;
        await board.restore_iroh_access(board_id);
      } else {
        received_pending[board_id] = device_id;
        await load_connection_details();
        if (request_approval) {
          toast.success(m.ui_access_request_sent());
        }
      }
    } catch (e) {
      logger.error("iroh.access_request.failed", e);
      if (await board.remove_revoked_iroh_board(board_id, e)) {
        delete received_pending[board_id];
        stopped_waiting.add(board_id);
        if (received_waiting_id === board_id) {
          received_waiting_id = null;
        }
        return;
      }
      if (
        !request_approval &&
        [
          "SHARE_ACCESS_REVOKED",
          "INVITE_DISABLED",
          "INVITE_NOT_FOUND",
        ].includes(parseCommandError(e)?.code ?? "")
      ) {
        delete received_pending[board_id];
        if (received_waiting_id === board_id) {
          received_waiting_id = null;
        }
        board.iroh_access_removed[board_id] = true;
        board.iroh_access_error[board_id] = parseCommandError(e) ?? {
          code: "INTERNAL_ERROR",
        };
      } else if (request_approval) {
        received_waiting_id = null;
        toast.error(translateCommandError(e));
      }
    } finally {
      received_requesting.delete(board_id);
      if (
        received_retry_request.delete(board_id) &&
        !stopped_waiting.has(board_id)
      ) {
        void request_received_access(board_id, true);
      }
    }
  }
  async function copy_device_id(id: string) {
    try {
      await navigator.clipboard.writeText(id);
      toast.success(m.ui_device_id_copied());
    } catch (e) {
      logger.warn("iroh.device_id_copy.failed", e);
      toast.error(m.ui_couldn_t_copy_device_id());
    }
  }
  async function update(
    invite: Invite,
    change: { permission?: "viewer" | "editor"; enabled?: boolean },
  ) {
    if (updating_invite_id) {
      return;
    }
    updating_invite_id = invite.invite_id;
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
      if (change.enabled === true && host_error) {
        toast.error(m.ui_invitation_saved_but_sharing_is_offline());
      } else {
        toast.success(m.ui_invitation_updated());
      }
    } catch (e) {
      logger.error("iroh.invite_update.failed", e);
      await load();
      toast.error(translateCommandError(e));
    } finally {
      updating_invite_id = null;
    }
  }
  async function remove() {
    if (!pending_delete) {
      return;
    }
    try {
      await invoke("delete_iroh_invite", {
        inviteId: pending_delete.invite_id,
      });
      await board.get_boards();
      await load();
      toast.success(m.ui_invitation_deleted());
    } catch (e) {
      logger.error("iroh.invite_delete.failed", e);
      toast.error(translateCommandError(e));
    } finally {
      pending_delete = null;
      delete_confirm_open = false;
    }
  }
  async function access(invite: Invite): Promise<InviteAccess | null> {
    if (invite_access[invite.invite_id]) {
      return invite_access[invite.invite_id];
    }
    loading_access_id = invite.invite_id;
    try {
      const value = await invoke<InviteAccess>("get_iroh_invite_access", {
        inviteId: invite.invite_id,
      });
      invite_access = { ...invite_access, [invite.invite_id]: value };
      return value;
    } catch (e) {
      logger.error("iroh.invite_access_load.failed", e);
      toast.error(translateCommandError(e));
      return null;
    } finally {
      loading_access_id = null;
    }
  }
  async function copy_invite(invite: Invite) {
    const value = await access(invite);
    if (!value) {
      return;
    }
    try {
      await navigator.clipboard.writeText(value.ticket);
      toast.success(m.ui_invitation_copied());
    } catch (e) {
      logger.warn("iroh.invite_copy.failed", e);
      toast.error(m.ui_couldn_t_copy_invitation());
    }
  }
  async function toggle_qr(invite: Invite) {
    if (visible_qr_id === invite.invite_id) {
      visible_qr_id = null;
      return;
    }
    if (await access(invite)) {
      visible_qr_id = invite.invite_id;
    }
  }
</script>

{#snippet invitation_identity(invite_id: string)}
  <div class="grid min-w-0 gap-1 text-xs text-muted-foreground">
    <span>{m.share_invitation_id()}</span>
    <code class="break-all select-all">{invite_id}</code>
  </div>
{/snippet}

{#snippet dialog_body()}
  <div
    class={(flow === "manage" || flow === "share") &&
    received_waiting_id === null
      ? "flex min-h-0 flex-1 flex-col gap-4"
      : flow === "home"
        ? "grid gap-2"
        : "grid gap-4"}
  >
    {#if flow !== "home" && !waiting_device_id && received_waiting_id === null && !(flow === "manage" && managed_board)}
      <Button
        variant="outline"
        size="sm"
        class="w-fit shrink-0"
        onclick={() => (flow === "share" ? leave_share() : (flow = "home"))}
        ><ArrowLeftIcon />{flow === "share" && share_return_to_manage
          ? managed_board
            ? m.share_back_to_board()
            : m.share_back_to_overview()
          : m.share_back_to_sharing()}</Button
      >
    {/if}
    {#if received_waiting_id !== null || waiting_device_id}
      {@const device_id =
        waiting_device_id ?? received_pending[received_waiting_id!]}
      <Card.Root
        class="items-center gap-4 border-primary/25 bg-primary/5 px-5 py-7 text-center shadow-none"
        role="status"
        ><div
          class="flex size-12 items-center justify-center rounded-full bg-primary/10 text-primary"
        >
          <LoaderCircleIcon class="size-6 animate-spin" />
        </div>
        <div class="space-y-1">
          <Card.Title>{m.ui_waiting_for_approval()}</Card.Title
          ><Card.Description
            >{m.share_device_wait_description()}</Card.Description
          >
        </div>
        {#if device_id}
          <div class="w-full rounded-lg border bg-background p-3 text-left">
            <p class="mb-2 text-xs font-medium text-muted-foreground">
              {m.ui_your_device_id_compare_this_with_the_owner()}
            </p>
            <code class="block break-all text-sm leading-relaxed select-all"
              >{verification_id(device_id)}</code
            >
          </div>
          <Button
            variant="outline"
            onclick={() => void copy_device_id(device_id)}
            ><ClipboardIcon />
            {m.ui_copy_device_id()}</Button
          >
        {/if}
        <div class="w-full text-left">
          <IrohConnectionInfo
            info={received_waiting_id !== null
              ? connection_details.boards[received_waiting_id]
              : connection_details.invitation}
          />
          {#if connection_details_error}
            <p class="mt-2 text-xs text-muted-foreground">
              {translateCommandError(connection_details_error)}
            </p>
          {/if}
        </div></Card.Root
      >
      <Button
        variant="outline"
        onclick={() => {
          if (received_waiting_id !== null) {
            stopped_waiting.add(received_waiting_id);
            delete received_pending[received_waiting_id];
            received_waiting_id = null;
          } else {
            wait_generation++;
            waiting_device_id = null;
            board.iroh_last_error = null;
          }
        }}>{m.ui_cancel_waiting()}</Button
      >
    {:else if flow === "home"}
      {#if !board.iroh_enabled}
        <p class="text-muted-foreground text-sm">
          {m.share_iroh_disabled()}
        </p>
      {/if}
      <div class="grid gap-3 sm:grid-cols-2">
        <button
          class="rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onclick={() => open_share()}
        >
          <Card.Root class="h-full gap-2 p-4 transition-colors hover:bg-accent"
            ><UsersIcon class="size-5 text-primary" />
            <Card.Title class="text-base">{m.ui_share_a_board()}</Card.Title
            ><Card.Description
              >{m.ui_choose_a_board_and_give_people_viewing_or_editing_access()}</Card.Description
            ></Card.Root
          >
        </button><button
          class="rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onclick={open_join}
        >
          <Card.Root class="h-full gap-2 p-4 transition-colors hover:bg-accent"
            ><LinkIcon class="size-5 text-primary" />
            <Card.Title class="text-base"
              >{m.ui_join_a_shared_board()}</Card.Title
            ><Card.Description
              >{m.ui_paste_an_invitation_link_to_join()}</Card.Description
            ></Card.Root
          >
        </button>
      </div>
      <Button
        variant="outline"
        onclick={() => {
          void load();
          flow = "manage";
        }}
        ><Settings2Icon />
        {m.ui_manage_sharing()}</Button
      >
      <Button variant="outline" onclick={open_network_settings}
        ><Settings2Icon />
        {m.share_connection_settings()}</Button
      >
    {:else if flow === "share"}
      <ScrollArea
        class="min-h-0 [&>[data-slot=scroll-area-viewport]]:max-h-[calc(100dvh-15rem)]"
        orientation="vertical"
        scrollbarYClasses="w-2"
      >
        <div class="grid gap-4 pr-3 pb-1">
          <label class="grid gap-2 text-sm font-medium"
            >{m.board_title()}<Select.Root
              type="single"
              disabled={creating}
              value={selected_board_id?.toString() ?? ""}
              onValueChange={(v) => {
                selected_board_id = v ? Number(v) : null;
                invalidate();
              }}
              ><Select.Trigger class="w-full"
                >{selected_board?.name ?? m.ui_choose_a_board()}</Select.Trigger
              ><Select.Content
                >{#each owned_boards as item}
                  <Select.Item value={item.id.toString()}
                    >{item.name}</Select.Item
                  >
                {/each}</Select.Content
              ></Select.Root
            ></label
          ><label class="grid gap-2 text-sm font-medium"
            >{m.ui_permission()}<Select.Root
              type="single"
              disabled={creating}
              value={permission}
              onValueChange={(v) => {
                permission = v as "viewer" | "editor";
                invalidate();
              }}
              ><Select.Trigger class="w-full"
                >{permission === "viewer"
                  ? m.board_read_only()
                  : m.board_can_edit()}</Select.Trigger
              ><Select.Content
                ><Select.Item value="viewer">{m.board_read_only()}</Select.Item
                ><Select.Item value="editor">{m.board_can_edit()}</Select.Item
                ></Select.Content
              ></Select.Root
            ></label
          >
          {#if generated}
            <Card.Root class="gap-3 bg-muted/30 p-4 shadow-none"
              ><div class="flex items-center justify-between gap-2">
                <Card.Title class="text-base">{generated.board_name}</Card.Title
                ><Badge variant="secondary"
                  >{generated.permission === "viewer"
                    ? m.board_read_only()
                    : m.board_can_edit()}</Badge
                >
              </div>
              {#if generated.qr}<div
                  class="qr mx-auto aspect-square w-full max-w-56 overflow-hidden rounded-md bg-white p-2"
                >
                  {@html generated.qr}
                </div>
              {:else if qr_loading}
                <p
                  class="flex items-center gap-2 text-sm text-muted-foreground"
                  role="status"
                >
                  <LoaderCircleIcon
                    class="size-4 animate-spin"
                  />{m.share_qr_generating()}
                </p>
              {/if}
              {#if qr_error}
                <p class="text-sm text-muted-foreground" role="status">
                  {m.share_qr_failed({
                    error: translateCommandError(qr_error),
                  })}
                </p>
                <Button variant="outline" onclick={() => void generate_qr()}
                  >{m.share_qr_retry()}</Button
                >
              {/if}
              <Button variant="outline" onclick={() => void copy()}
                >{m.ui_copy_invitation_link()}</Button
              ><Card.Description
                >{m.share_device_invite_description()}</Card.Description
              ></Card.Root
            >
          {/if}
          {#if create_error}
            <p
              class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
              role="alert"
            >
              {translateCommandError(create_error)}
            </p>
          {/if}
          <Button
            onclick={() => void create()}
            disabled={creating || selected_board_id === null}
            >{creating
              ? m.ui_creating()
              : generated
                ? m.ui_create_new_invitation()
                : m.ui_create_invitation()}</Button
          >
        </div>
      </ScrollArea>
    {:else if flow === "join"}
      <Card.Root class="gap-4 border-primary/20 bg-muted/20 p-5 shadow-none"
        ><div class="flex items-center gap-3">
          <span
            class="flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary"
            ><LinkIcon class="size-5" /></span
          >
          <div>
            <Card.Title class="text-base">{m.ui_invitation_link()}</Card.Title
            ><Card.Description
              >{m.ui_paste_the_link_from_the_board_owner()}</Card.Description
            >
          </div>
        </div>
        <Input
          bind:value={ticket}
          placeholder="cardbe://share/..."
          aria-label={m.ui_invitation_link()}
          oninput={() => (board.iroh_last_error = null)}
        /></Card.Root
      >
      {#if board.iroh_last_error}
        <p
          class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
          role="alert"
        >
          {translateCommandError(board.iroh_last_error)}
        </p>
      {/if}
      <Button onclick={() => void join()} disabled={!ticket.trim() || joining}
        >{joining ? m.ui_connecting() : m.ui_request_access()}</Button
      >
    {:else}
      {#if !managed_board}
        <ButtonGroup.Root
          class="grid shrink-0 grid-cols-2"
          role="group"
          aria-label={m.ui_sharing_management_views()}
        >
          <Button
            bind:ref={manage_view_button}
            variant={manage_view === "mine" ? "secondary" : "outline"}
            aria-pressed={manage_view === "mine"}
            onclick={() => {
              manage_view = "mine";
              managed_board_id = null;
              visible_qr_id = null;
            }}>{m.share_owned_count({ count: shared_boards.length })}</Button
          >
          <Button
            variant={manage_view === "received" ? "secondary" : "outline"}
            aria-pressed={manage_view === "received"}
            onclick={() => {
              manage_view = "received";
              managed_board_id = null;
              visible_qr_id = null;
            }}
            >{m.share_received_count({
              count: received_boards.length,
            })}</Button
          >
        </ButtonGroup.Root>
      {/if}
      <div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
        <p class="text-xs text-muted-foreground">
          {managed_board
            ? m.share_board_summary({
                invites: managed_board.invites.length,
                devices: managed_board.approved,
              })
            : manage_view === "mine"
              ? m.share_manage_auto_help()
              : ""}
        </p>
        <Button
          size="sm"
          variant="outline"
          class="shrink-0"
          onclick={() => {
            if (manage_view === "mine") {
              open_share();
            } else {
              open_join();
            }
          }}
          ><PlusIcon />{managed_board
            ? m.ui_create_new_invitation()
            : manage_view === "mine"
              ? m.ui_share_a_board()
              : m.ui_join_a_shared_board()}</Button
        >
      </div>
      <ScrollArea
        bind:viewport={manage_viewport}
        class="min-h-0 flex-1"
        orientation="vertical"
        scrollbarYClasses="w-2"
      >
        <div class="grid gap-3 pr-3 pb-1">
          {#if manage_error}
            <div
              class="flex flex-wrap items-center justify-between gap-2 rounded-md border border-destructive/40 p-3 text-sm text-destructive"
              role="alert"
            >
              <p>{translateCommandError(manage_error)}</p>
              <Button
                variant="outline"
                size="sm"
                disabled={loading_invites}
                onclick={() => void load()}
              >
                {loading_invites
                  ? m.share_network_retrying()
                  : m.common_retry()}
              </Button>
            </div>
          {/if}
          {#if connection_details_error}
            <p class="text-xs text-muted-foreground" role="status">
              {translateCommandError(connection_details_error)}
            </p>
          {/if}
          {#if manage_view === "mine" && host_error}
            <p
              class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
              role="alert"
            >
              {m.share_service_unavailable()}
            </p>
          {/if}
          {#if manage_view === "received"}
            {#if received_boards.length === 0}
              <Card.Root class="p-4 shadow-none"
                ><Card.Description
                  >{m.ui_no_boards_have_been_shared_with_you()}</Card.Description
                ><Button variant="outline" onclick={open_join}
                  >{m.ui_join_a_shared_board()}</Button
                ></Card.Root
              >
            {:else}
              {#each received_boards as shared (shared.id)}
                <IrohReceivedBoardCard
                  {board}
                  {shared}
                  pending_device_id={received_pending[shared.id]}
                  connection={connection_details.boards[shared.id]}
                  connection_dialog_open={connection_dialog_id === shared.id}
                  connection_ticket={connection_tickets[shared.id] ?? ""}
                  connection_error={connection_update_errors[shared.id]}
                  saving_connection={saving_connection_id !== null}
                  saving_this_connection={saving_connection_id === shared.id}
                  onopen={() => {
                    void board.switch_board(shared.id);
                    open = false;
                  }}
                  onshowwaiting={() => (received_waiting_id = shared.id)}
                  onrequestaccess={() =>
                    void request_received_access(shared.id, true)}
                  onconnectiondialogchange={(value) =>
                    (connection_dialog_id = value ? shared.id : null)}
                  onticketinput={(value) => {
                    connection_tickets[shared.id] = value;
                    delete connection_update_errors[shared.id];
                  }}
                  onsaveconnection={() => void update_connection(shared.id)}
                />
              {/each}
            {/if}
          {:else if loading_invites && invites.length === 0}
            <p
              class="flex items-center gap-2 p-4 text-sm text-muted-foreground"
              role="status"
            >
              <LoaderCircleIcon
                class="size-4 animate-spin"
              />{m.share_loading_invitations()}
            </p>
          {:else if invites.length === 0}
            <Card.Root class="p-4 shadow-none"
              ><Card.Title class="text-base">{m.share_first_board()}</Card.Title
              >
              <Card.Description
                >{m.share_first_board_description()}</Card.Description
              ><Button onclick={() => open_share()}
                >{m.ui_share_a_board()}</Button
              ></Card.Root
            >
          {:else}
            {#if !managed_board}
              {#if pending_requests.length > 0}
                <section
                  class="grid gap-2 rounded-lg border border-primary/25 bg-primary/5 p-3"
                  aria-label={m.share_waiting_count({
                    count: pending_count,
                  })}
                >
                  <h3 class="text-sm font-semibold">
                    {m.share_waiting_count({ count: pending_count })}
                  </h3>
                  {#each pending_requests as request (request.invite.invite_id + request.device.node_id)}
                    <div
                      class="flex items-center gap-3 rounded-md border bg-background p-3"
                    >
                      <div class="min-w-0 flex-1">
                        <p class="truncate text-sm font-medium">
                          {request.invite.board_name}
                        </p>
                        <p class="text-xs text-muted-foreground">
                          {request.invite.permission === "viewer"
                            ? m.board_read_only()
                            : m.board_can_edit()} · {invitation_date(
                            request.invite.created_at,
                          )}
                        </p>
                        <code class="text-xs text-muted-foreground"
                          >{request.device.node_id.slice(
                            0,
                            8,
                          )}…{request.device.node_id.slice(-8)}</code
                        >
                        {@render invitation_identity(request.invite.invite_id)}
                      </div>
                      <Button
                        size="sm"
                        aria-haspopup="dialog"
                        onclick={(event) =>
                          open_device_review(
                            request.invite,
                            request.device.node_id,
                            event.currentTarget,
                          )}>{m.ui_review()}</Button
                      >
                    </div>
                  {/each}
                </section>
              {/if}
              {#each shared_boards as item (item.board_id)}
                <Button
                  variant="outline"
                  class="h-auto w-full justify-between gap-3 p-4 text-left whitespace-normal"
                  onclick={() => void open_managed_board(item.board_id)}
                >
                  <span class="grid min-w-0 gap-1"
                    ><span class="break-words font-semibold"
                      >{item.board_name}</span
                    ><span class="text-xs font-normal text-muted-foreground"
                      >{m.share_board_summary({
                        invites: item.invites.length,
                        devices: item.approved,
                      })}</span
                    ></span
                  ><ChevronRightIcon class="shrink-0" />
                </Button>
              {/each}
            {:else}
              {#each visible_invites as invite (invite.invite_id)}
                <IrohInviteCard
                  {invite}
                  updating={updating_invite_id !== null}
                  loading_access={loading_access_id === invite.invite_id}
                  qr_visible={visible_qr_id === invite.invite_id}
                  qr_svg={visible_qr_id === invite.invite_id
                    ? invite_access[invite.invite_id]?.qr_svg
                    : undefined}
                  devices_open={expanded_devices[invite.invite_id]}
                  device_connections={connection_details.devices}
                  onupdate={(change) => void update(invite, change)}
                  oncopy={() => void copy_invite(invite)}
                  ontoggleqr={() => void toggle_qr(invite)}
                  ondelete={() => {
                    pending_delete = invite;
                    delete_confirm_open = true;
                  }}
                  ondevicesopenchange={(value) =>
                    (expanded_devices = {
                      ...expanded_devices,
                      [invite.invite_id]: value,
                    })}
                  onreview={(node_id, trigger) =>
                    open_device_review(invite, node_id, trigger)}
                  onrevoke={(node_id) => {
                    revoke_candidate = { invite, node_id };
                    revoke_open = true;
                  }}
                />
              {/each}
            {/if}
          {/if}
        </div>
      </ScrollArea>
    {/if}
  </div>
{/snippet}

<Dialog.Root bind:open>
  <Dialog.Content
    showCloseButton={!waiting_device_id && received_waiting_id === null}
    interactOutsideBehavior={waiting_device_id ||
    received_waiting_id !== null ||
    approval_open ||
    revoke_open ||
    delete_confirm_open
      ? "ignore"
      : "close"}
    onEscapeKeydown={(event) => {
      if (waiting_device_id || received_waiting_id !== null) {
        event.preventDefault();
      } else if (flow === "manage" && managed_board) {
        event.preventDefault();
        void back_to_overview();
      } else if (flow === "share") {
        event.preventDefault();
        void leave_share();
      }
    }}
    class={flow === "manage" && received_waiting_id === null
      ? "flex h-[min(42rem,calc(100dvh-2rem))] w-[calc(100vw-2rem)] flex-col overflow-hidden sm:max-w-xl"
      : "flex max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] flex-col overflow-hidden sm:max-w-xl"}
  >
    {#if flow === "manage" && managed_board && received_waiting_id === null}
      <Dialog.Header class="shrink-0 gap-3 border-b pb-4 text-left">
        <Button
          variant="outline"
          size="sm"
          class="w-fit"
          onclick={() => void back_to_overview()}
          ><ArrowLeftIcon />{m.share_back_to_overview()}</Button
        >
        <div class="min-w-0 space-y-2 pr-6">
          <p class="text-xs text-muted-foreground">
            {m.share_shared_board_label()}
          </p>
          <Dialog.Title
            bind:ref={detail_heading}
            tabindex={-1}
            class="break-words rounded-sm leading-snug focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >{managed_board.board_name}</Dialog.Title
          >
        </div>
      </Dialog.Header>
    {:else}
      <Dialog.Header class="shrink-0">
        <Dialog.Title
          >{received_waiting_id !== null
            ? m.ui_waiting_for_approval()
            : flow === "home"
              ? m.ui_board_sharing()
              : flow === "share"
                ? m.ui_share_a_board()
                : flow === "join"
                  ? m.ui_join_a_shared_board()
                  : m.ui_manage_board_sharing()}</Dialog.Title
        >
        {#if flow === "home"}
          <Dialog.Description
            >{m.ui_collaborate_in_cardbe_with_read_only_or_editing_access()}</Dialog.Description
          >
        {:else if flow === "manage" && received_waiting_id === null}
          <Dialog.Description
            >{m.ui_manage_boards_you_share_and_boards_shared_with_you()}</Dialog.Description
          >
        {/if}
      </Dialog.Header>
    {/if}

    {#if (flow === "manage" || flow === "share") && received_waiting_id === null && !waiting_device_id}
      {@render dialog_body()}
    {:else}
      <ScrollArea class="min-h-0" orientation="vertical">
        {@render dialog_body()}
      </ScrollArea>
    {/if}
  </Dialog.Content>
</Dialog.Root>
<AlertDialog.Root bind:open={delete_confirm_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title>{m.ui_delete_invitation()}</AlertDialog.Title
      ><AlertDialog.Description
        >{m.share_invite_delete_description()}</AlertDialog.Description
      ></AlertDialog.Header
    >
    {#if pending_delete}
      <p class="break-words text-sm font-medium">{pending_delete.board_name}</p>
      {@render invitation_identity(pending_delete.invite_id)}
    {/if}
    <AlertDialog.Footer
      ><AlertDialog.Cancel
        onclick={() => {
          pending_delete = null;
          delete_confirm_open = false;
        }}>{m.common_cancel()}</AlertDialog.Cancel
      ><AlertDialog.Action variant="destructive" onclick={() => void remove()}
        >{m.ui_delete_invitation()}</AlertDialog.Action
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
<AlertDialog.Root bind:open={approval_open}
  ><AlertDialog.Content
    onCloseAutoFocus={(event) => {
      event.preventDefault();
      void restore_review_focus();
    }}
    onEscapeKeydown={(event) => {
      if (device_action_pending) {
        event.preventDefault();
      }
    }}
    ><AlertDialog.Header
      ><AlertDialog.Title>{m.ui_review_device_request()}</AlertDialog.Title
      ><AlertDialog.Description
        >{m.share_review_description()}</AlertDialog.Description
      ></AlertDialog.Header
    >
    {#if approval_candidate}
      {@render invitation_identity(approval_candidate.invite.invite_id)}
      <div class="flex flex-wrap items-center gap-2 text-sm">
        <span class="font-medium">{approval_candidate.invite.board_name}</span>
        <Badge variant="secondary"
          >{approval_candidate.invite.permission === "viewer"
            ? m.board_read_only()
            : m.board_can_edit()}</Badge
        >
      </div>
      {#if !approval_candidate.invite.enabled}
        <p class="text-sm text-muted-foreground">
          {m.share_disabled_invitation_approval_help()}
        </p>
      {/if}
      <div class="rounded-lg border bg-muted/30 p-3">
        <div class="mb-2 text-xs font-medium text-muted-foreground">
          {m.ui_device_id()}
        </div>
        <code class="block break-all text-sm leading-relaxed select-all"
          >{verification_id(approval_candidate.node_id)}</code
        ><Button
          variant="outline"
          size="sm"
          class="mt-3"
          onclick={() => void copy_device_id(approval_candidate!.node_id)}
          ><ClipboardIcon />
          {m.ui_copy_id()}</Button
        >
      </div>
    {/if}
    <div class="flex items-start gap-2 rounded-md border p-3">
      <Checkbox
        id="device-id-verified"
        class="mt-0.5 disabled:opacity-100"
        bind:checked={device_id_verified}
        disabled={device_action_pending}
      />
      <Label
        for="device-id-verified"
        class="block text-sm leading-5 peer-disabled:opacity-100"
        >{m.share_device_id_verified()}</Label
      >
    </div>
    <AlertDialog.Footer
      ><AlertDialog.Cancel
        class="disabled:opacity-100"
        disabled={device_action_pending}>{m.ui_back()}</AlertDialog.Cancel
      ><Button
        variant="destructive"
        class="disabled:opacity-100"
        disabled={device_action_pending}
        onclick={() => {
          if (approval_candidate) {
            void approve_device(
              approval_candidate.invite,
              approval_candidate.node_id,
              false,
            );
          }
        }}>{m.ui_reject()}</Button
      ><Button
        class={device_id_verified
          ? "min-w-36 disabled:opacity-100"
          : "min-w-36"}
        aria-busy={device_action_pending}
        disabled={device_action_pending || !device_id_verified}
        onclick={() => {
          if (approval_candidate) {
            void approve_device(
              approval_candidate.invite,
              approval_candidate.node_id,
              true,
            );
          }
        }}>{m.ui_approve_device()}</Button
      ></AlertDialog.Footer
    ></AlertDialog.Content
  ></AlertDialog.Root
>
<AlertDialog.Root bind:open={revoke_open}
  ><AlertDialog.Content
    onEscapeKeydown={(event) => {
      if (device_action_pending) {
        event.preventDefault();
      }
    }}
    ><AlertDialog.Header
      ><AlertDialog.Title>{m.ui_revoke_device_access()}</AlertDialog.Title
      ><AlertDialog.Description
        >{m.share_revoke_description({
          name: revoke_candidate?.invite.board_name ?? m.ui_this_board(),
        })}</AlertDialog.Description
      ></AlertDialog.Header
    >
    {#if revoke_candidate}
      <code
        class="block break-all rounded-md border bg-muted/30 p-3 text-sm select-all"
        >{revoke_candidate.node_id}</code
      >
    {/if}
    <AlertDialog.Footer
      ><AlertDialog.Cancel
        class="disabled:opacity-100"
        disabled={device_action_pending}>{m.common_cancel()}</AlertDialog.Cancel
      ><Button
        variant="destructive"
        class="disabled:opacity-100"
        disabled={device_action_pending}
        onclick={() => void revoke_device()}>{m.ui_revoke_access()}</Button
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
