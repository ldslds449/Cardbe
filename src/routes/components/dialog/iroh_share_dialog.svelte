<script lang="ts">
  import { registerUnsavedChanges } from "$lib/unsaved-changes";
  import { formatNumber } from "$lib/i18n";
  import { getLocale } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import {
    type CommandError,
    parseCommandError,
    translateCommandError,
  } from "$lib/command-errors";
  import { logger } from "$lib/logger";
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
  import XIcon from "@lucide/svelte/icons/x";
  import { onMount, tick, untrack } from "svelte";
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
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Textarea } from "$lib/components/ui/textarea/index.js";
  import type { BoardStore, BoardSummary } from "../../board.svelte";
  import {
    IrohDeviceStatus,
    type IrohInvite,
    type IrohConnectionInfo,
    type IrohConnectionDetails,
  } from "../../type/iroh-share";

  type Flow = "home" | "share" | "join" | "manage" | "network";
  type Invite = IrohInvite;
  type NetworkSettingsField =
    | "listen_port"
    | "direct_addresses"
    | "relay_urls"
    | "discovery_urls";
  type Generated = {
    ticket: string;
    qr: string;
    board_name: string;
    permission: "viewer" | "editor";
  };
  type InviteAccess = { ticket: string; qr_svg: string };
  type IrohNetworkSettings = {
    direct_ip_enabled: boolean;
    discovery_enabled: boolean;
    discovery_urls: string[];
    relay_enabled: boolean;
    relay_urls: string[];
    direct_addresses: string[];
    listen_port: number;
  };
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
  let manage_view_button = $state<HTMLButtonElement | null>(null);
  let host_error = $state<unknown>(null);
  let pending_only = $state(false);
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
  const visible_invites = $derived(
    invites
      .filter(
        (invite) =>
          !pending_only ||
          invite.devices.some(
            (device) => device.status === IrohDeviceStatus.Pending,
          ),
      )
      .sort(
        (a, b) =>
          Number(
            b.devices.some(
              (device) => device.status === IrohDeviceStatus.Pending,
            ),
          ) -
          Number(
            a.devices.some(
              (device) => device.status === IrohDeviceStatus.Pending,
            ),
          ),
      ),
  );
  function verification_id(id: string) {
    return id.match(/.{1,4}/g)?.join(" ") ?? id;
  }
  function invitation_date(value: string) {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? m.share_creation_date_unavailable()
      : m.share_created_date({ date: sync_time.format(date) });
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
  let network_settings = $state<IrohNetworkSettings>({
    direct_ip_enabled: true,
    discovery_enabled: true,
    discovery_urls: [],
    relay_enabled: true,
    relay_urls: [],
    direct_addresses: [],
    listen_port: 0,
  });
  let relay_urls_text = $state("");
  let discovery_urls_text = $state("");
  let direct_addresses_text = $state("");
  let saving_network_settings = $state(false);
  let loading_network_settings = $state(false);
  let saved_network_settings = $state("");
  let network_settings_applied = $state(false);
  let network_restart_failed = $state(false);
  let discard_network_open = $state(false);
  let close_after_discard = false;
  const network_draft = $derived(
    JSON.stringify({
      direct_ip_enabled: network_settings.direct_ip_enabled,
      discovery_enabled: network_settings.discovery_enabled,
      relay_enabled: network_settings.relay_enabled,
      relay_urls: split_addresses(relay_urls_text),
      discovery_urls: split_addresses(discovery_urls_text),
      direct_addresses: split_addresses(direct_addresses_text),
      listen_port: network_settings.listen_port ?? 0,
    }),
  );
  const network_dirty = $derived(network_draft !== saved_network_settings);
  const network_has_transport = $derived(
    network_settings.direct_ip_enabled || network_settings.relay_enabled,
  );
  const network_mode = $derived(
    network_settings.direct_ip_enabled && network_settings.relay_enabled
      ? "automatic"
      : network_settings.direct_ip_enabled && !network_settings.relay_enabled
        ? "direct"
        : !network_settings.direct_ip_enabled && network_settings.relay_enabled
          ? "relay"
          : "",
  );
  const network_mode_label = $derived(
    network_mode === "automatic"
      ? m.share_network_automatic()
      : network_mode === "direct"
        ? m.share_network_direct()
        : network_mode === "relay"
          ? m.share_network_relay()
          : m.share_network_choose(),
  );
  function set_network_mode(mode: string) {
    if (!["automatic", "direct", "relay"].includes(mode)) {
      return;
    }
    network_settings.direct_ip_enabled = mode !== "relay";
    network_settings.relay_enabled = mode !== "direct";
    clear_network_settings_error();
  }
  const network_customized = $derived(
    Boolean(
      relay_urls_text.trim() ||
      discovery_urls_text.trim() ||
      direct_addresses_text.trim() ||
      network_settings.listen_port,
    ),
  );
  function leave_network(discard = false, close = false) {
    if (saving_network_settings) {
      return;
    }
    if (
      !discard &&
      flow === "network" &&
      !loading_network_settings &&
      network_dirty
    ) {
      close_after_discard = close;
      discard_network_open = true;
      return;
    }
    discard_network_open = false;
    if (close) {
      open = false;
    }
    flow = "home";
  }
  function reset_network_settings() {
    network_settings = {
      direct_ip_enabled: true,
      discovery_enabled: true,
      relay_enabled: true,
      direct_addresses: [],
      listen_port: 0,
      relay_urls: [],
      discovery_urls: [],
    };
    relay_urls_text = discovery_urls_text = direct_addresses_text = "";
    clear_network_settings_error();
  }
  let network_settings_error = $state<unknown>(null);
  let network_settings_field_errors = $state<
    Partial<Record<NetworkSettingsField, CommandError | "port_range">>
  >({});
  let advanced_addresses_open = $state(false);
  let waiting_device_id = $state<string | null>(null);
  let checking_approval = false;
  let wait_generation = 0;
  let approval_candidate = $state<{ invite: Invite; node_id: string } | null>(
    null,
  );
  let approval_open = $state(false);
  let revoke_candidate = $state<{ invite: Invite; node_id: string } | null>(
    null,
  );
  let revoke_open = $state(false);
  let received_pending = $state<Record<number, string>>({});
  let received_waiting_id = $state<number | null>(null);
  const sync_time = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      dateStyle: "medium",
      timeStyle: "short",
    }),
  );
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
    const unregister_draft = registerUnsavedChanges(
      () =>
        open &&
        flow === "network" &&
        !loading_network_settings &&
        network_dirty,
    );
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
      unregister_draft();
      window.clearInterval(timer);
    };
  });
  $effect(() => {
    if (open && (show_requests || show_received)) {
      const view = show_received ? "received" : "mine";
      untrack(() => {
        flow = "manage";
        manage_view = view;
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
    board.iroh_last_error = null;
    flow = "join";
  }
  function split_addresses(value: string) {
    return value
      .split(/[\n,]/)
      .map((item) => item.trim())
      .filter(Boolean);
  }
  function error_message(error: unknown, _fallback: string) {
    return translateCommandError(error);
  }
  function clear_network_settings_error(field?: NetworkSettingsField) {
    if (field) {
      network_settings_field_errors[field] = undefined;
    } else {
      network_settings_field_errors = {};
    }
    network_settings_error = null;
  }
  async function set_network_settings_error(error: unknown) {
    const message = error_message(error, m.share_network_save_error());
    clear_network_settings_error();
    const parsed = parseCommandError(error);
    network_settings_error = parsed ?? {
      code: "INTERNAL_ERROR",
    };
    const field =
      parsed?.code === "NETWORK_SETTINGS_INVALID" ? parsed.field : undefined;
    if (parsed?.code === "NETWORK_SETTINGS_INVALID" && field) {
      network_settings_field_errors[field] = parsed;
      advanced_addresses_open = true;
      await tick();
      const id = {
        listen_port: "network-listen-port",
        direct_addresses: "network-direct-addresses",
        relay_urls: "network-relay-urls",
        discovery_urls: "network-discovery-urls",
      }[field];
      const input = document.getElementById(id);
      input?.focus();
      input?.scrollIntoView({ block: "nearest" });
    }
    return message;
  }
  async function open_network_settings() {
    if (loading_network_settings) {
      return;
    }
    loading_network_settings = true;
    try {
      clear_network_settings_error();
      network_settings = await invoke<IrohNetworkSettings>(
        "get_iroh_network_settings",
      );
      relay_urls_text = network_settings.relay_urls.join("\n");
      discovery_urls_text = network_settings.discovery_urls.join("\n");
      direct_addresses_text = network_settings.direct_addresses.join("\n");
      saved_network_settings = network_draft;
      network_settings_applied = false;
      advanced_addresses_open = false;
      if (open) {
        flow = "network";
      }
    } catch (e) {
      logger.error("iroh.network_settings_load.failed", e);
      toast.error(error_message(e, m.share_network_load_error()));
    } finally {
      loading_network_settings = false;
    }
  }
  async function save_network_settings() {
    if (
      saving_network_settings ||
      !network_has_transport ||
      (!network_dirty && !network_restart_failed)
    ) {
      return;
    }
    const settings = JSON.parse(network_draft) as IrohNetworkSettings;
    if (
      !Number.isInteger(settings.listen_port) ||
      settings.listen_port < 0 ||
      settings.listen_port > 65535
    ) {
      clear_network_settings_error();
      network_settings_field_errors.listen_port = "port_range";
      advanced_addresses_open = true;
      await tick();
      document.getElementById("network-listen-port")?.focus();
      return;
    }
    saving_network_settings = true;
    network_settings_applied = false;
    clear_network_settings_error();
    try {
      const restart_error = await invoke<unknown>("set_iroh_network_settings", {
        settings,
      });
      saved_network_settings = JSON.stringify(settings);
      invite_access = {};
      visible_qr_id = null;
      generated = null;
      network_restart_failed = restart_error !== null;
      network_settings_applied = restart_error === null;
      network_settings_error = restart_error;
      if (restart_error === null) {
        board.reconnect_iroh_boards();
        await load_connection_details();
      }
    } catch (e) {
      logger.error("iroh.network_settings_save.failed", e);
      await set_network_settings_error(e);
    } finally {
      saving_network_settings = false;
    }
  }
  async function create() {
    if (selected_board_id === null) {
      return;
    }
    creating = true;
    try {
      const created = await board.create_iroh_invite(
        selected_board_id,
        permission,
      );
      generated = {
        ticket: created,
        qr: await invoke<string>("iroh_invite_qr_svg", { ticket: created }),
        board_name: selected_board?.name ?? m.board_title(),
        permission,
      };
      let copied = false;
      try {
        await navigator.clipboard.writeText(created);
        copied = true;
      } catch (e) {
        logger.warn("iroh.invite_create_copy.failed", e);
        /* The visible link can still be copied later. */
      }
      await load();
      toast.success(
        copied ? m.ui_invitation_copied() : m.ui_invitation_created(),
      );
    } catch (e) {
      logger.error("iroh.invite_create.failed", e);
      toast.error(translateCommandError(e));
    } finally {
      creating = false;
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

{#snippet connection_info(info: IrohConnectionInfo | null | undefined)}
  <Collapsible.Root class="rounded-md border bg-background">
    <Collapsible.Trigger
      class="group flex w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-xs font-medium hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span>{m.share_connection_last()}</span>
      <span class="flex items-center gap-2">
        {#if info && info.paths.length > 0}
          <Badge variant="outline"
            >{[
              ...new Set(
                info.paths.map((path) =>
                  path.kind === "Direct"
                    ? m.share_direct_ip()
                    : path.kind === "Relay"
                      ? m.share_relay_servers()
                      : m.share_custom_transport(),
                ),
              ),
            ].join(" + ")}</Badge
          >
        {/if}
        <ChevronDownIcon
          class="size-3.5 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
        />
      </span>
    </Collapsible.Trigger>
    <Collapsible.Content class="space-y-2 border-t px-3 py-3 text-xs">
      {#if info}
        <p class="text-muted-foreground">
          {m.share_connection_observed({
            time: sync_time.format(new Date(info.observed_at)),
          })}{#if waiting_device_id || received_waiting_id !== null}
            {m.share_approval_reconnect()}{/if}
        </p>
        {#if info.paths.length > 0}
          <Collapsible.Root>
            <Collapsible.Trigger
              class="group flex items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {m.share_connection_paths({
                count: formatNumber(info.paths.length),
              })}
              <ChevronDownIcon
                class="size-3.5 transition-transform group-data-[state=open]:rotate-180"
              />
            </Collapsible.Trigger>
            <Collapsible.Content class="space-y-3 pt-2">
              {#each [...new Set(info.paths.map((path) => path.kind))] as kind}
                <div class="space-y-1">
                  <p class="font-medium">
                    {kind === "Direct"
                      ? m.share_direct_addresses()
                      : kind === "Relay"
                        ? m.share_relay_servers()
                        : m.share_custom_transport()}
                  </p>
                  <code
                    class="block whitespace-pre-wrap break-all rounded-md bg-muted px-2 py-1 select-all"
                    >{[
                      ...new Set(
                        info.paths
                          .filter((path) => path.kind === kind)
                          .map((path) => path.address),
                      ),
                    ].join("\n")}</code
                  >
                </div>
              {/each}
              <p class="text-muted-foreground">
                Multiple addresses can belong to the same device. Direct IP may
                use NAT traversal; Iroh does not report whether hole punching
                was required.
              </p>
            </Collapsible.Content>
          </Collapsible.Root>
        {:else}
          <p class="text-muted-foreground">
            Iroh has not reported the active path yet.
          </p>
        {/if}
      {:else}
        <p class="text-muted-foreground">
          No connection observed this session. Connect or sync to collect
          details.
        </p>
      {/if}
    </Collapsible.Content>
  </Collapsible.Root>
{/snippet}

<Dialog.Root bind:open>
  <Dialog.Content
    showCloseButton={flow !== "network" &&
      !saving_network_settings &&
      !waiting_device_id &&
      received_waiting_id === null}
    interactOutsideBehavior={waiting_device_id ||
    received_waiting_id !== null ||
    approval_open ||
    revoke_open ||
    delete_confirm_open
      ? "ignore"
      : "close"}
    onInteractOutside={(event) => {
      if (flow === "network") {
        event.preventDefault();
        if (!discard_network_open) {
          leave_network(false, true);
        }
      }
    }}
    onEscapeKeydown={(event) => {
      if (
        saving_network_settings ||
        waiting_device_id ||
        received_waiting_id !== null
      ) {
        event.preventDefault();
      } else if (flow === "network") {
        event.preventDefault();
        if (!discard_network_open) {
          leave_network(false, true);
        }
      }
    }}
    class={(flow === "manage" || flow === "network") &&
    received_waiting_id === null
      ? "flex h-[min(42rem,calc(100dvh-2rem))] w-[calc(100vw-2rem)] flex-col overflow-hidden sm:max-w-xl"
      : flow === "share" && received_waiting_id === null
        ? "flex max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] flex-col overflow-hidden sm:max-w-xl"
        : "max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] overflow-y-auto sm:max-w-xl"}
  >
    {#if flow === "network"}
      <Button
        variant="ghost"
        size="icon"
        class="absolute right-3 top-3 size-7"
        disabled={saving_network_settings}
        aria-label={m.share_back_to_sharing()}
        title={m.share_back_to_sharing()}
        onclick={() => leave_network(false, true)}
        ><XIcon class="size-4" /></Button
      >
    {/if}
    <Dialog.Header class={flow === "network" ? "pr-6" : ""}>
      <Dialog.Title
        >{received_waiting_id !== null
          ? m.ui_waiting_for_approval()
          : flow === "home"
            ? m.ui_board_sharing()
            : flow === "share"
              ? m.ui_share_a_board()
              : flow === "join"
                ? m.ui_join_a_shared_board()
                : flow === "network"
                  ? m.share_connection_settings()
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
      {:else if flow === "network"}
        <Dialog.Description>{m.share_network_description()}</Dialog.Description>
      {/if}
    </Dialog.Header>
    <div
      class={(flow === "manage" || flow === "network" || flow === "share") &&
      received_waiting_id === null
        ? "flex min-h-0 flex-1 flex-col gap-4"
        : flow === "home"
          ? "grid gap-2"
          : "grid gap-4"}
    >
      {#if flow !== "home" && !waiting_device_id && received_waiting_id === null}
        <Button
          variant="outline"
          size="sm"
          class="w-fit"
          disabled={saving_network_settings}
          onclick={() =>
            flow === "network" ? leave_network() : (flow = "home")}
          ><ArrowLeftIcon />{m.share_back_to_sharing()}</Button
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
            {@render connection_info(
              received_waiting_id !== null
                ? connection_details.boards[received_waiting_id]
                : connection_details.invitation,
            )}
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
        <div class="grid gap-3 sm:grid-cols-2">
          <button
            class="rounded-lg text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onclick={() => (flow = "share")}
          >
            <Card.Root
              class="h-full gap-2 p-4 transition-colors hover:bg-accent"
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
            <Card.Root
              class="h-full gap-2 p-4 transition-colors hover:bg-accent"
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
        <Button
          variant="outline"
          disabled={loading_network_settings}
          onclick={() => void open_network_settings()}
          ><Settings2Icon />
          {loading_network_settings
            ? m.share_loading_settings()
            : m.share_connection_settings()}</Button
        >
      {:else if flow === "network"}
        <ScrollArea
          class="min-h-0 flex-1"
          orientation="vertical"
          type="always"
          scrollbarYClasses="w-2"
        >
          <fieldset
            disabled={saving_network_settings}
            class="grid min-w-0 gap-4 pr-4"
          >
            <Card.Root class="gap-0 p-0 shadow-none">
              <Card.Header class="px-4 py-3">
                <Card.Title class="text-base"
                  >{m.share_network_method()}</Card.Title
                >
                <Card.Description
                  >{m.share_network_compatibility()}</Card.Description
                >
              </Card.Header>
              <Card.Content class="grid gap-2 px-4 pb-4">
                <Label for="network-mode">{m.share_network_mode()}</Label>
                <Select.Root
                  type="single"
                  value={network_mode}
                  onValueChange={set_network_mode}
                >
                  <Select.Trigger
                    id="network-mode"
                    class="w-full"
                    aria-describedby="network-mode-help"
                    >{network_mode_label}</Select.Trigger
                  >
                  <Select.Content>
                    <Select.Item value="automatic"
                      >{m.share_network_automatic()}</Select.Item
                    >
                    <Select.Item value="direct"
                      >{m.share_network_direct()}</Select.Item
                    >
                    <Select.Item value="relay"
                      >{m.share_network_relay()}</Select.Item
                    >
                  </Select.Content>
                </Select.Root>
                <p id="network-mode-help" class="text-sm text-muted-foreground">
                  {#if network_mode === "automatic"}
                    {m.share_network_automatic_help()}
                  {:else if network_mode === "direct"}
                    {m.share_network_direct_help()}
                  {:else if network_mode === "relay"}
                    {m.share_network_relay_help()}
                  {:else}
                    {m.share_network_disabled_help()}
                  {/if}
                </p>
              </Card.Content>
            </Card.Root>
            <Collapsible.Root
              bind:open={advanced_addresses_open}
              class="rounded-lg border bg-card"
            >
              <Collapsible.Trigger
                class="group flex w-full items-center gap-2 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                <span class="flex-1">{m.share_network_advanced()}</span>
                {#if network_customized}<Badge variant="secondary"
                    >{m.share_network_customized()}</Badge
                  >{/if}
                <ChevronDownIcon
                  class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
                />
              </Collapsible.Trigger>
              <Collapsible.Content class="border-t p-4">
                <div class="grid gap-5">
                  <div class="grid gap-2">
                    <Label for="network-discovery"
                      >{m.share_network_addresses()}</Label
                    >
                    <Select.Root
                      type="single"
                      value={network_settings.discovery_enabled
                        ? "automatic"
                        : "manual"}
                      onValueChange={(value) => {
                        network_settings.discovery_enabled =
                          value === "automatic";
                        clear_network_settings_error("discovery_urls");
                      }}
                    >
                      <Select.Trigger
                        id="network-discovery"
                        class="w-full"
                        aria-describedby="network-discovery-help"
                        >{network_settings.discovery_enabled
                          ? m.share_network_find()
                          : m.share_network_manual()}</Select.Trigger
                      >
                      <Select.Content>
                        <Select.Item value="automatic"
                          >{m.share_network_find()}</Select.Item
                        >
                        <Select.Item value="manual"
                          >{m.share_network_manual()}</Select.Item
                        >
                      </Select.Content>
                    </Select.Root>
                    <p
                      id="network-discovery-help"
                      class="text-sm text-muted-foreground"
                    >
                      {m.share_network_discovery_help()}
                    </p>
                  </div>

                  <label class="grid gap-2 text-sm font-medium">
                    {m.share_network_port()}
                    <Input
                      id="network-listen-port"
                      type="number"
                      min={0}
                      max={65535}
                      step={1}
                      bind:value={network_settings.listen_port}
                      disabled={!network_settings.direct_ip_enabled}
                      aria-describedby={"network-listen-port-help" +
                        (network_settings_field_errors.listen_port
                          ? " network-listen-port-error"
                          : "")}
                      aria-invalid={Boolean(
                        network_settings_field_errors.listen_port,
                      )}
                      oninput={() =>
                        clear_network_settings_error("listen_port")}
                    />
                    <span
                      id="network-listen-port-help"
                      class="font-normal text-muted-foreground"
                    >
                      {m.share_network_port_help()}
                    </span>
                    {#if network_settings_field_errors.listen_port}
                      <span
                        id="network-listen-port-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{network_settings_field_errors.listen_port ===
                        "port_range"
                          ? m.share_network_port_error()
                          : translateCommandError(
                              network_settings_field_errors.listen_port,
                            )}</span
                      >
                    {/if}
                  </label>
                  <label class="grid gap-2 text-sm font-medium">
                    {m.share_network_fixed_addresses()}
                    <Textarea
                      id="network-direct-addresses"
                      aria-describedby={"network-direct-addresses-help" +
                        (network_settings_field_errors.direct_addresses
                          ? " network-direct-addresses-error"
                          : "")}
                      bind:value={direct_addresses_text}
                      disabled={!network_settings.direct_ip_enabled}
                      aria-invalid={Boolean(
                        network_settings_field_errors.direct_addresses,
                      )}
                      oninput={() =>
                        clear_network_settings_error("direct_addresses")}
                      rows={3}
                      placeholder="192.168.1.20:12345"
                    />
                    <span
                      id="network-direct-addresses-help"
                      class="font-normal text-muted-foreground"
                      >{m.share_network_fixed_help()}</span
                    >
                    <span class="text-xs font-normal text-muted-foreground"
                      >{#if !network_settings.direct_ip_enabled}{m.share_network_enable_direct()}{:else if !direct_addresses_text.trim()}{m.share_network_auto_addresses()}{/if}</span
                    >
                    {#if network_settings_field_errors.direct_addresses}
                      <span
                        id="network-direct-addresses-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{translateCommandError(
                          network_settings_field_errors.direct_addresses,
                        )}</span
                      >
                    {/if}
                  </label>
                  <label class="grid gap-2 text-sm font-medium">
                    {m.share_network_relay_urls()}
                    <Textarea
                      id="network-relay-urls"
                      aria-describedby={"network-relay-urls-help" +
                        (network_settings_field_errors.relay_urls
                          ? " network-relay-urls-error"
                          : "")}
                      bind:value={relay_urls_text}
                      disabled={!network_settings.relay_enabled}
                      aria-invalid={Boolean(
                        network_settings_field_errors.relay_urls,
                      )}
                      oninput={() => clear_network_settings_error("relay_urls")}
                      rows={3}
                      placeholder="https://relay.example.com"
                    />
                    <span
                      id="network-relay-urls-help"
                      class="font-normal text-muted-foreground"
                      >{m.share_network_relay_help_urls()}</span
                    >
                    <span class="text-xs font-normal text-muted-foreground"
                      >{#if !network_settings.relay_enabled}{m.share_network_enable_relay()}{:else if !relay_urls_text.trim()}{m.share_network_default_service()}{/if}</span
                    >
                    {#if network_settings_field_errors.relay_urls}
                      <span
                        id="network-relay-urls-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{translateCommandError(
                          network_settings_field_errors.relay_urls,
                        )}</span
                      >
                    {/if}
                  </label>
                  <label class="grid gap-2 text-sm font-medium">
                    {m.share_network_discovery_urls()}
                    <Textarea
                      id="network-discovery-urls"
                      aria-describedby={"network-discovery-urls-help" +
                        (network_settings_field_errors.discovery_urls
                          ? " network-discovery-urls-error"
                          : "")}
                      bind:value={discovery_urls_text}
                      disabled={!network_settings.discovery_enabled}
                      aria-invalid={Boolean(
                        network_settings_field_errors.discovery_urls,
                      )}
                      oninput={() =>
                        clear_network_settings_error("discovery_urls")}
                      rows={3}
                      placeholder="https://discovery.example.com"
                    />
                    <span
                      id="network-discovery-urls-help"
                      class="font-normal text-muted-foreground"
                      >{m.share_network_discovery_help_urls()}</span
                    >
                    <span class="text-xs font-normal text-muted-foreground"
                      >{#if !network_settings.discovery_enabled}{m.share_network_enable_discovery()}{:else if !discovery_urls_text.trim()}{m.share_network_default_service()}{/if}</span
                    >
                    {#if network_settings_field_errors.discovery_urls}
                      <span
                        id="network-discovery-urls-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{translateCommandError(
                          network_settings_field_errors.discovery_urls,
                        )}</span
                      >
                    {/if}
                  </label>
                </div>
              </Collapsible.Content>
            </Collapsible.Root>
          </fieldset>
        </ScrollArea>
        <div class="shrink-0 space-y-3 border-t pt-3">
          {#if !network_has_transport}
            <p class="text-sm text-destructive" role="alert">
              {m.share_network_enable_transport()}
            </p>
          {/if}
          {#if network_settings_error}
            <p
              class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
              role="alert"
            >
              {translateCommandError(network_settings_error)}
            </p>
          {/if}
          <p class="text-xs text-muted-foreground" role="status">
            {saving_network_settings
              ? m.share_network_applying_changes()
              : network_dirty
                ? m.share_network_unapplied()
                : network_restart_failed
                  ? m.share_network_retry_activate()
                  : network_settings_applied
                    ? m.share_network_applied()
                    : m.share_network_no_changes()}
          </p>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <Button
              variant="outline"
              disabled={saving_network_settings}
              onclick={reset_network_settings}
              >{m.share_network_defaults()}</Button
            >
            <div class="flex gap-2">
              <Button
                variant="outline"
                class="min-w-24"
                disabled={saving_network_settings}
                onclick={() => leave_network(false, true)}
                >{m.common_cancel()}</Button
              >
              <Button
                class="min-w-24"
                aria-busy={saving_network_settings}
                onclick={() => void save_network_settings()}
                disabled={saving_network_settings ||
                  !network_has_transport ||
                  (!network_dirty && !network_restart_failed)}
              >
                {saving_network_settings
                  ? m.share_network_applying()
                  : network_restart_failed && !network_dirty
                    ? m.common_retry()
                    : m.share_network_apply()}
              </Button>
            </div>
          </div>
        </div>
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
                value={selected_board_id?.toString() ?? ""}
                onValueChange={(v) => {
                  selected_board_id = v ? Number(v) : null;
                  invalidate();
                }}
                ><Select.Trigger class="w-full"
                  >{selected_board?.name ??
                    m.ui_choose_a_board()}</Select.Trigger
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
                  ><Select.Item value="viewer"
                    >{m.board_read_only()}</Select.Item
                  ><Select.Item value="editor">{m.board_can_edit()}</Select.Item
                  ></Select.Content
                ></Select.Root
              ></label
            >
            {#if generated}
              <Card.Root class="gap-3 bg-muted/30 p-4 shadow-none"
                ><div class="flex items-center justify-between gap-2">
                  <Card.Title class="text-base"
                    >{generated.board_name}</Card.Title
                  ><Badge variant="secondary"
                    >{generated.permission === "viewer"
                      ? m.board_read_only()
                      : m.board_can_edit()}</Badge
                  >
                </div>
                <div
                  class="qr mx-auto aspect-square w-full max-w-56 overflow-hidden rounded-md bg-white p-2"
                >
                  {@html generated.qr}
                </div>
                <Button variant="outline" onclick={() => void copy()}
                  >{m.ui_copy_invitation_link()}</Button
                ><Card.Description
                  >{m.share_device_invite_description()}</Card.Description
                ></Card.Root
              >
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
        <ButtonGroup.Root
          class="grid grid-cols-2"
          role="group"
          aria-label={m.ui_sharing_management_views()}
        >
          <Button
            bind:ref={manage_view_button}
            variant={manage_view === "mine" ? "secondary" : "outline"}
            aria-pressed={manage_view === "mine"}
            onclick={() => (manage_view = "mine")}
            >{m.share_owned_count({ count: invites.length })}</Button
          >
          <Button
            variant={manage_view === "received" ? "secondary" : "outline"}
            aria-pressed={manage_view === "received"}
            onclick={() => (manage_view = "received")}
            >{m.share_received_count({ count: received_boards.length })}</Button
          >
        </ButtonGroup.Root>
        <div
          class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
        >
          {#if manage_view === "mine"}
            <div class="flex items-center gap-2">
              <Checkbox id="pending-devices-only" bind:checked={pending_only} />
              <Label for="pending-devices-only" class="text-xs"
                >{m.share_pending_filter({
                  count: formatNumber(pending_count),
                })}</Label
              >
            </div>
          {:else}<span>{m.share_owner_sync()}</span>{/if}
        </div>
        <p class="text-xs text-muted-foreground">
          {manage_view === "mine"
            ? m.share_manage_auto_help()
            : m.share_received_auto_help()}
        </p>
        <ScrollArea
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
                          >{board.iroh_access_removed[shared.id]
                            ? board.iroh_access_error[shared.id]
                              ? translateCommandError(
                                  board.iroh_access_error[shared.id],
                                )
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
                                  new Date(
                                    board.iroh_last_synced_at[shared.id],
                                  ),
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
                      <Button
                        variant="outline"
                        onclick={() => {
                          void board.switch_board(shared.id);
                          open = false;
                        }}>{m.ui_open_board()}</Button
                      >
                      {#if shared.sync_status === "conflict"}
                        <Button
                          variant="outline"
                          onclick={() =>
                            void board.resolve_iroh_conflict(shared.id, false)}
                          >{m.ui_use_owner_version()}</Button
                        >
                        {#if shared.shared_role === "editor"}
                          <Button
                            variant="outline"
                            onclick={() =>
                              void board.resolve_iroh_conflict(shared.id, true)}
                            >{m.ui_keep_my_version()}</Button
                          >
                        {/if}
                      {:else}
                        <Button
                          variant="outline"
                          disabled={shared.sync_status === "syncing" ||
                            !!board.iroh_access_removed[shared.id] ||
                            !!received_pending[shared.id]}
                          onclick={() => void board.sync_iroh_board(shared.id)}
                          >{#if shared.sync_status === "syncing"}
                            <LoaderCircleIcon class="animate-spin" />
                            {m.share_syncing()}
                          {:else}
                            {m.share_sync()}
                          {/if}</Button
                        >
                      {/if}
                    </div>
                    {#if received_pending[shared.id]}
                      <Button
                        variant="secondary"
                        class="w-full"
                        onclick={() => (received_waiting_id = shared.id)}
                        >{m.ui_waiting_for_approval_view_device_id()}</Button
                      >
                    {:else if board.iroh_access_removed[shared.id]}
                      <div
                        class="grid gap-2 rounded-lg border border-primary/25 bg-primary/5 p-3 text-sm"
                      >
                        <span
                          >{board.iroh_access_error[shared.id]
                            ? translateCommandError(
                                board.iroh_access_error[shared.id],
                              )
                            : m.share_access_unavailable()}
                          {m.share_access_removed_help()}</span
                        ><Button
                          class="w-full"
                          onclick={() =>
                            void request_received_access(shared.id, true)}
                          >{m.ui_request_access_again()}</Button
                        >
                      </div>
                    {/if}
                    {@render connection_info(
                      connection_details.boards[shared.id],
                    )}
                    {#if board.iroh_sync_error[shared.id] && !board.iroh_access_removed[shared.id]}
                      <p class="text-sm text-destructive" role="status">
                        {translateCommandError(
                          board.iroh_sync_error[shared.id],
                        )}
                      </p>
                    {/if}
                  </Card.Root>
                {/each}
              {/if}
            {:else if loading_invites && invites.length === 0}
              <p
                class="flex items-center gap-2 p-4 text-sm text-muted-foreground"
                role="status"
              >
                <LoaderCircleIcon class="size-4 animate-spin" />Loading
                invitations...
              </p>
            {:else if invites.length === 0}
              <Card.Root class="p-4 shadow-none"
                ><Card.Title class="text-base"
                  >{m.share_first_board()}</Card.Title
                >
                <Card.Description
                  >{m.share_first_board_description()}</Card.Description
                ></Card.Root
              >
            {:else}
              {#if pending_count > 0}
                <div
                  class="flex items-center gap-3 rounded-lg border border-primary/25 bg-primary/5 p-3 text-sm"
                >
                  <ShieldCheckIcon class="size-5 text-primary" />
                  <span class="flex-1"
                    >{m.share_waiting_count({ count: pending_count })}</span
                  >
                </div>
              {/if}
              {#if visible_invites.length === 0}<p
                  class="rounded-md border p-4 text-sm text-muted-foreground"
                >
                  {m.share_no_pending()}
                </p>{/if}
              {#each visible_invites as invite (invite.invite_id)}
                <Card.Root class="gap-3 p-4 shadow-none">
                  <div
                    class="flex flex-wrap items-center justify-between gap-2"
                  >
                    <div>
                      <Card.Title class="text-base"
                        >{invite.board_name}</Card.Title
                      ><Card.Description
                        >{invitation_date(invite.created_at)}</Card.Description
                      >
                    </div>
                    <Badge variant={invite.enabled ? "secondary" : "outline"}
                      >{invite.enabled
                        ? m.explorer_active()
                        : m.ui_disabled()}</Badge
                    >
                  </div>
                  <Collapsible.Root class="rounded-md border">
                    <Collapsible.Trigger
                      class="group flex w-full items-center justify-between gap-2 px-3 py-2 text-sm hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <span
                        >{m.share_invitation_settings({
                          permission:
                            invite.permission === "viewer"
                              ? m.board_read_only()
                              : m.board_can_edit(),
                        })}</span
                      >
                      <ChevronDownIcon
                        class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
                      />
                    </Collapsible.Trigger>
                    <Collapsible.Content class="space-y-2 border-t p-3">
                      <div class="flex flex-wrap items-center gap-3">
                        <Select.Root
                          type="single"
                          disabled={updating_invite_id !== null}
                          value={invite.permission}
                          onValueChange={(v) =>
                            void update(invite, {
                              permission: v as "viewer" | "editor",
                            })}
                        >
                          <Select.Trigger
                            class="w-36"
                            aria-label={m.share_permission_for({
                              name: invite.board_name,
                            })}
                            >{invite.permission === "viewer"
                              ? m.board_read_only()
                              : m.board_can_edit()}</Select.Trigger
                          >
                          <Select.Content
                            ><Select.Item value="viewer"
                              >{m.board_read_only()}</Select.Item
                            ><Select.Item value="editor"
                              >{m.board_can_edit()}</Select.Item
                            ></Select.Content
                          >
                        </Select.Root>
                        <div class="flex items-center gap-2 px-1">
                          <Switch
                            id={`invite-active-${invite.invite_id}`}
                            disabled={updating_invite_id !== null}
                            checked={invite.enabled}
                            onCheckedChange={(checked) =>
                              void update(invite, { enabled: checked })}
                          />
                          <Label for={`invite-active-${invite.invite_id}`}
                            >{m.share_enabled()}</Label
                          >
                        </div>
                      </div>
                      <p class="text-xs text-muted-foreground">
                        {m.share_settings_help()}
                      </p>
                    </Collapsible.Content>
                  </Collapsible.Root>
                  <div class="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      disabled={!invite.enabled ||
                        loading_access_id === invite.invite_id}
                      onclick={() => void copy_invite(invite)}
                      >{m.ui_copy_link()}</Button
                    >
                    <Button
                      variant="outline"
                      disabled={!invite.enabled ||
                        loading_access_id === invite.invite_id}
                      onclick={() => void toggle_qr(invite)}
                      >{visible_qr_id === invite.invite_id
                        ? m.ui_hide_qr()
                        : m.ui_show_qr()}</Button
                    >
                    <Button
                      class="ml-auto"
                      variant="destructive"
                      onclick={() => {
                        pending_delete = invite;
                        delete_confirm_open = true;
                      }}>{m.common_delete()}</Button
                    >
                  </div>
                  {#if invite.devices.length > 0}
                    <Collapsible.Root
                      open={expanded_devices[invite.invite_id] ??
                        invite.devices.some(
                          (device) =>
                            device.status === IrohDeviceStatus.Pending,
                        )}
                      onOpenChange={(value) =>
                        (expanded_devices = {
                          ...expanded_devices,
                          [invite.invite_id]: value,
                        })}
                      class="rounded-lg border bg-background"
                    >
                      <Collapsible.Trigger
                        class="group flex w-full items-center gap-2 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      >
                        <ShieldCheckIcon class="size-4 text-muted-foreground" />
                        <span class="flex-1"
                          >{m.ui_devices()}
                          <span class="text-muted-foreground"
                            >({formatNumber(invite.devices.length)})</span
                          ></span
                        >
                        {#if invite.devices.some((device) => device.status === IrohDeviceStatus.Pending)}
                          <Badge variant="secondary"
                            >{m.share_pending_count({
                              count: invite.devices.filter(
                                (device) =>
                                  device.status === IrohDeviceStatus.Pending,
                              ).length,
                            })}</Badge
                          >
                        {/if}
                        <ChevronDownIcon
                          class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
                        />
                      </Collapsible.Trigger>
                      <Collapsible.Content class="border-t">
                        <div class="space-y-2 p-2">
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
                                    {device.status === IrohDeviceStatus.Pending
                                      ? m.ui_approval_requested()
                                      : device.status ===
                                          IrohDeviceStatus.Approved
                                        ? m.ui_approved_device()
                                        : m.share_access_revoked()}
                                  </div>
                                  <div
                                    class="font-mono text-xs text-muted-foreground"
                                  >
                                    {device.node_id.slice(
                                      0,
                                      8,
                                    )}…{device.node_id.slice(-8)}
                                  </div>
                                </div>
                                {#if device.status === IrohDeviceStatus.Pending}
                                  <Button
                                    size="sm"
                                    onclick={() => {
                                      approval_candidate = {
                                        invite,
                                        node_id: device.node_id,
                                      };
                                      device_id_verified = false;
                                      approval_open = true;
                                    }}
                                    aria-haspopup="dialog"
                                    >{m.ui_review()}</Button
                                  >
                                {:else if device.status === IrohDeviceStatus.Approved}
                                  <Button
                                    variant="outline"
                                    size="sm"
                                    onclick={() => {
                                      revoke_candidate = {
                                        invite,
                                        node_id: device.node_id,
                                      };
                                      revoke_open = true;
                                    }}>{m.ui_revoke()}</Button
                                  >
                                {/if}
                              </div>
                              <div class="mt-2">
                                {@render connection_info(
                                  connection_details.devices[device.node_id],
                                )}
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
                      aria-label={m.ui_board_invitation_qr_code()}
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
<AlertDialog.Root bind:open={discard_network_open}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{m.share_network_discard_title()}</AlertDialog.Title>
      <AlertDialog.Description
        >{m.share_network_discard_help()}</AlertDialog.Description
      >
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{m.unsaved_changes_keep_editing()}</AlertDialog.Cancel
      >
      <AlertDialog.Action
        variant="destructive"
        onclick={() => leave_network(true, close_after_discard)}
        >{m.share_network_discard()}</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
<AlertDialog.Root bind:open={delete_confirm_open}
  ><AlertDialog.Content
    ><AlertDialog.Header
      ><AlertDialog.Title>{m.ui_delete_invitation()}</AlertDialog.Title
      ><AlertDialog.Description
        >{m.share_invite_delete_description()}</AlertDialog.Description
      ></AlertDialog.Header
    ><AlertDialog.Footer
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
      manage_view_button?.focus({ preventScroll: true });
    }}
    onEscapeKeydown={(event) => {
      if (device_action_pending) {
        event.preventDefault();
      }
    }}
    ><AlertDialog.Header
      ><AlertDialog.Title>{m.ui_review_device_request()}</AlertDialog.Title
      ><AlertDialog.Description
        >{m.share_review_description({
          permission:
            approval_candidate?.invite.permission === "editor"
              ? m.board_can_edit()
              : m.board_read_only(),
        })}</AlertDialog.Description
      ></AlertDialog.Header
    >
    {#if approval_candidate}
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
          This invitation is disabled. Approval will be saved, but sharing must
          be enabled before the device can sync.
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
