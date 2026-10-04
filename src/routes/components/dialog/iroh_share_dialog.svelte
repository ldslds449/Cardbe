<script lang="ts">
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
  let host_error = $state<string | null>(null);
  let pending_only = $state(false);
  let loading_invites = $state(false);
  let manage_error = $state<string | null>(null);
  let device_action_pending = $state(false);
  let device_id_verified = $state(false);
  let updating_invite_id = $state<string | null>(null);
  let connection_details = $state<IrohConnectionDetails>({
    invitation: null,
    devices: {},
    boards: {},
  });
  let connection_details_error = $state<string | null>(null);
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
      ? "Creation date unavailable"
      : `Created ${sync_time.format(date)}`;
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
      connection_details_error =
        "Connection details are unavailable. They will update on the next check.";
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
      ? "Automatic (recommended)"
      : network_mode === "direct"
        ? "Direct only"
        : network_mode === "relay"
          ? "Relay only"
          : "Choose a connection mode",
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
  function leave_network(discard = false) {
    if (saving_network_settings) {
      return;
    }
    if (!discard && flow === "network" && network_dirty) {
      discard_network_open = true;
      return;
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
  let network_settings_error = $state<string | null>(null);
  let network_settings_field_errors = $state<
    Partial<Record<NetworkSettingsField, string>>
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
    return () => window.clearInterval(timer);
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
  function split_addresses(value: string) {
    return value
      .split(/[\n,]/)
      .map((item) => item.trim())
      .filter(Boolean);
  }
  function error_message(error: unknown, fallback: string) {
    if (error instanceof Error && error.message) {
      return error.message;
    }
    if (typeof error === "string" && error.trim()) {
      return error;
    }
    if (
      error &&
      typeof error === "object" &&
      "message" in error &&
      typeof error.message === "string" &&
      error.message
    ) {
      return error.message;
    }
    return fallback;
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
    const message = error_message(error, "Couldn't save connection settings");
    clear_network_settings_error();
    if (message.startsWith("Invalid listening port:")) {
      network_settings_field_errors.listen_port = message;
    } else if (message.startsWith("Invalid direct address:")) {
      network_settings_field_errors.direct_addresses = `${message}. Enter IP:port, for example 192.168.1.20:12345.`;
    } else if (message.startsWith("Invalid relay URL:")) {
      network_settings_field_errors.relay_urls = `${message}. Enter a URL such as https://relay.example.com.`;
    } else if (message.startsWith("Invalid discovery service URL:")) {
      network_settings_field_errors.discovery_urls = `${message}. Enter a compatible discovery service URL.`;
    } else {
      network_settings_error = message;
    }
    const field = Object.keys(network_settings_field_errors)[0] as
      | NetworkSettingsField
      | undefined;
    if (field) {
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
      toast.error(error_message(e, "Couldn't load connection settings"));
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
      await set_network_settings_error(
        "Invalid listening port: enter a whole number from 0 to 65535.",
      );
      return;
    }
    saving_network_settings = true;
    network_settings_applied = false;
    clear_network_settings_error();
    try {
      const restart_error = await invoke<string | null>(
        "set_iroh_network_settings",
        {
          settings,
        },
      );
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
        board_name: selected_board?.name ?? "Board",
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
      toast.success(copied ? "Invitation copied" : "Invitation created");
    } catch (e) {
      logger.error("iroh.invite_create.failed", e);
      toast.error(
        e instanceof Error ? e.message : "Couldn't create invitation",
      );
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
      toast.success("Invitation copied");
    } catch (e) {
      logger.warn("iroh.invite_copy.failed", e);
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
        await load_connection_details();
      } else if (
        silent &&
        board.iroh_last_error.startsWith("Access was declined or revoked")
      ) {
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
      host_error = await invoke<string | null>("iroh_host_error");
      onRequestsChanged();
      manage_error = null;
      await load_connection_details();
    } catch (e) {
      logger.error("iroh.invite_list_load.failed", e);
      manage_error = error_message(e, "Couldn't load sharing. Try again.");
      if (!silent) {
        toast.error(
          e instanceof Error ? e.message : "Couldn't load invitations",
        );
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
          ? "Device approved"
          : invite.devices.find((device) => device.node_id === node_id)
                ?.status === IrohDeviceStatus.Pending
            ? "Request rejected"
            : "Device access revoked",
      );
      return true;
    } catch (e) {
      logger.error("iroh.device_approval.failed", e);
      toast.error(e instanceof Error ? e.message : "Couldn't update device");
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
          toast.success("Access request sent");
        }
      }
    } catch (e) {
      logger.error("iroh.access_request.failed", e);
      if (
        !request_approval &&
        /^(ACCESS_REVOKED:|INVITATION_DISABLED:|INVITATION_DELETED:|Access was declined or revoked)/.test(
          e instanceof Error ? e.message : String(e),
        )
      ) {
        delete received_pending[board_id];
        if (received_waiting_id === board_id) {
          received_waiting_id = null;
        }
        board.iroh_access_removed[board_id] = true;
        board.iroh_access_error[board_id] = (
          e instanceof Error ? e.message : String(e)
        ).replace(/^[A-Z_]+:/, "");
      } else if (request_approval) {
        received_waiting_id = null;
        toast.error(
          (e instanceof Error ? e.message : String(e)).replace(/^[A-Z_]+:/, ""),
        );
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
      toast.success("Device ID copied");
    } catch (e) {
      logger.warn("iroh.device_id_copy.failed", e);
      toast.error("Couldn't copy Device ID");
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
        toast.error("Invitation saved, but sharing is offline");
      } else {
        toast.success("Invitation updated");
      }
    } catch (e) {
      logger.error("iroh.invite_update.failed", e);
      await load();
      toast.error(
        e instanceof Error ? e.message : "Couldn't update invitation",
      );
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
      toast.success("Invitation deleted");
    } catch (e) {
      logger.error("iroh.invite_delete.failed", e);
      toast.error(
        e instanceof Error ? e.message : "Couldn't delete invitation",
      );
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
      toast.error(e instanceof Error ? e.message : "Couldn't load invitation");
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
      toast.success("Invitation copied");
    } catch (e) {
      logger.warn("iroh.invite_copy.failed", e);
      toast.error("Couldn't copy invitation");
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
      <span>Last used connection</span>
      <span class="flex items-center gap-2">
        {#if info && info.paths.length > 0}
          <Badge variant="outline"
            >{[
              ...new Set(
                info.paths.map((path) =>
                  path.kind === "Direct" ? "Direct IP" : path.kind,
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
          Observed at {sync_time.format(new Date(info.observed_at))}. This is
          the path used for the last connection, and may change on the next
          sync. Enabled connection options do not indicate which path was used.{#if waiting_device_id || received_waiting_id !== null}
            Approval checks reconnect automatically, so temporary ports and
            network paths can change.{/if}
        </p>
        {#if info.paths.length > 0}
          <Collapsible.Root>
            <Collapsible.Trigger
              class="group flex items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Technical details ({info.paths.length} network paths)
              <ChevronDownIcon
                class="size-3.5 transition-transform group-data-[state=open]:rotate-180"
              />
            </Collapsible.Trigger>
            <Collapsible.Content class="space-y-3 pt-2">
              {#each [...new Set(info.paths.map((path) => path.kind))] as kind}
                <div class="space-y-1">
                  <p class="font-medium">
                    {kind === "Direct"
                      ? "Direct IP addresses"
                      : kind === "Relay"
                        ? "Relay servers"
                        : "Custom transport"}
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
          leave_network();
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
          leave_network();
        }
      }
    }}
    class={(flow === "manage" || flow === "network") &&
    received_waiting_id === null
      ? "flex h-[min(42rem,calc(100dvh-2rem))] w-[calc(100vw-2rem)] flex-col overflow-hidden sm:max-w-xl"
      : "max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] overflow-y-auto sm:max-w-xl"}
  >
    {#if flow === "network"}
      <Button
        variant="ghost"
        size="icon"
        class="absolute right-3 top-3 size-7"
        disabled={saving_network_settings}
        aria-label="Back to board sharing"
        title="Back to board sharing"
        onclick={() => leave_network()}><XIcon class="size-4" /></Button
      >
    {/if}
    <Dialog.Header class={flow === "network" ? "pr-6" : ""}>
      <Dialog.Title
        >{received_waiting_id !== null
          ? "Waiting for approval"
          : flow === "home"
            ? "Board sharing"
            : flow === "share"
              ? "Share a board"
              : flow === "join"
                ? "Join a shared board"
                : flow === "network"
                  ? "Connection settings"
                  : "Manage board sharing"}</Dialog.Title
      >
      {#if flow === "home"}
        <Dialog.Description
          >Collaborate in Cardbe with read-only or editing access.</Dialog.Description
        >
      {:else if flow === "manage" && received_waiting_id === null}
        <Dialog.Description
          >Manage boards you share and boards shared with you.</Dialog.Description
        >
      {:else if flow === "network"}
        <Dialog.Description
          >Applies to all board sharing on this device. Applying changes may
          briefly interrupt connections. Received boards reconnect
          automatically; their local changes are kept.</Dialog.Description
        >
      {/if}
    </Dialog.Header>
    <div
      class={(flow === "manage" || flow === "network") &&
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
          ><ArrowLeftIcon />
          Back to sharing</Button
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
            <Card.Title>Waiting for approval</Card.Title><Card.Description
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
                >{verification_id(device_id)}</code
              >
            </div>
            <Button
              variant="outline"
              onclick={() => void copy_device_id(device_id)}
              ><ClipboardIcon />
              Copy device ID</Button
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
                {connection_details_error}
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
              board.iroh_last_error = "";
            }
          }}>Cancel waiting</Button
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
              <Card.Title class="text-base">Share a board</Card.Title
              ><Card.Description
                >Choose a board and give people viewing or editing access.</Card.Description
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
          onclick={() => {
            void load();
            flow = "manage";
          }}
          ><Settings2Icon />
          Manage sharing</Button
        >
        <Button
          variant="outline"
          disabled={loading_network_settings}
          onclick={() => void open_network_settings()}
          ><Settings2Icon />
          {loading_network_settings
            ? "Loading settings..."
            : "Connection settings"}</Button
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
                <Card.Title class="text-base">Connection method</Card.Title>
                <Card.Description
                  >Applies to this device. Both devices need a compatible
                  connection method.</Card.Description
                >
              </Card.Header>
              <Card.Content class="grid gap-2 px-4 pb-4">
                <Label for="network-mode">Connection mode</Label>
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
                      >Automatic (recommended)</Select.Item
                    >
                    <Select.Item value="direct">Direct only</Select.Item>
                    <Select.Item value="relay">Relay only</Select.Item>
                  </Select.Content>
                </Select.Root>
                <p id="network-mode-help" class="text-sm text-muted-foreground">
                  {#if network_mode === "automatic"}
                    Allows direct connections or encrypted relay connections
                    when needed. Recommended on both devices.
                  {:else if network_mode === "direct"}
                    Transfers data directly between devices. Both devices must
                    allow direct connections; firewalls and some networks can
                    block them. No relay fallback.
                  {:else if network_mode === "relay"}
                    Transfers encrypted data through a relay. The owner must
                    allow relay connections and their relay must be reachable.
                    Direct connections are disabled.
                  {:else}
                    Connections are disabled in your saved settings. Choose a
                    mode to allow devices to connect.
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
                <span class="flex-1">Advanced addresses and services</span>
                {#if network_customized}<Badge variant="secondary"
                    >Customized</Badge
                  >{/if}
                <ChevronDownIcon
                  class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
                />
              </Collapsible.Trigger>
              <Collapsible.Content class="border-t p-4">
                <div class="grid gap-5">
                  <div class="grid gap-2">
                    <Label for="network-discovery">Device addresses</Label>
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
                          ? "Find automatically (recommended)"
                          : "Use invitation and manual addresses"}</Select.Trigger
                      >
                      <Select.Content>
                        <Select.Item value="automatic"
                          >Find automatically (recommended)</Select.Item
                        >
                        <Select.Item value="manual"
                          >Use invitation and manual addresses</Select.Item
                        >
                      </Select.Content>
                    </Select.Root>
                    <p
                      id="network-discovery-help"
                      class="text-sm text-muted-foreground"
                    >
                      Automatic discovery finds updated addresses after a device
                      restarts or changes settings. Without it, you may need a
                      new invitation or updated manual addresses to reconnect.
                    </p>
                  </div>

                  <label class="grid gap-2 text-sm font-medium">
                    Listening UDP port
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
                      Use 0 for an automatic port. For port forwarding, set a
                      fixed port and forward UDP traffic to it.
                    </span>
                    {#if network_settings_field_errors.listen_port}
                      <span
                        id="network-listen-port-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{network_settings_field_errors.listen_port}</span
                      >
                    {/if}
                  </label>
                  <label class="grid gap-2 text-sm font-medium">
                    This device's fixed addresses
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
                      >Enter the IP:port other devices can reach, one per line.
                      Set a listening UDP port above; if your router uses a
                      different external port, enter that port here. Leave empty
                      for automatic addresses.</span
                    >
                    <span class="text-xs font-normal text-muted-foreground"
                      >{#if !network_settings.direct_ip_enabled}Enable direct
                        connection to edit this field.{:else if !direct_addresses_text.trim()}Using
                        automatic addresses.{/if}</span
                    >
                    {#if network_settings_field_errors.direct_addresses}
                      <span
                        id="network-direct-addresses-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{network_settings_field_errors.direct_addresses}</span
                      >
                    {/if}
                  </label>
                  <label class="grid gap-2 text-sm font-medium">
                    Relay service URLs
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
                      >One URL per line. Leave empty for the default service.
                      Custom URLs replace the default relay service.</span
                    >
                    <span class="text-xs font-normal text-muted-foreground"
                      >{#if !network_settings.relay_enabled}Enable relay
                        connection to edit this field.{:else if !relay_urls_text.trim()}Using
                        the default service.{/if}</span
                    >
                    {#if network_settings_field_errors.relay_urls}
                      <span
                        id="network-relay-urls-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{network_settings_field_errors.relay_urls}</span
                      >
                    {/if}
                  </label>
                  <label class="grid gap-2 text-sm font-medium">
                    Discovery service URLs
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
                      >One compatible service URL per line. Leave empty for the
                      default service. Custom URLs replace the default discovery
                      service.</span
                    >
                    <span class="text-xs font-normal text-muted-foreground"
                      >{#if !network_settings.discovery_enabled}Enable automatic
                        device discovery to edit this field.{:else if !discovery_urls_text.trim()}Using
                        the default service.{/if}</span
                    >
                    {#if network_settings_field_errors.discovery_urls}
                      <span
                        id="network-discovery-urls-error"
                        class="text-sm font-normal text-destructive"
                        role="alert"
                        >{network_settings_field_errors.discovery_urls}</span
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
              Enable direct or relay connections so devices can transfer data.
            </p>
          {/if}
          {#if network_settings_error}
            <p
              class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
              role="alert"
            >
              {network_settings_error}
            </p>
          {/if}
          <p class="text-xs text-muted-foreground" role="status">
            {saving_network_settings
              ? "Applying changes…"
              : network_dirty
                ? "Changes not applied"
                : network_restart_failed
                  ? "Settings saved. Retry to activate them."
                  : network_settings_applied
                    ? "Settings applied. Received boards are reconnecting automatically."
                    : "No unapplied changes"}
          </p>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <Button
              variant="outline"
              disabled={saving_network_settings}
              onclick={reset_network_settings}>Restore defaults</Button
            >
            <div class="flex gap-2">
              <Button
                variant="outline"
                class="min-w-24"
                disabled={saving_network_settings}
                onclick={() => leave_network(true)}>Cancel</Button
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
                  ? "Applying…"
                  : network_restart_failed && !network_dirty
                    ? "Retry"
                    : "Apply"}
              </Button>
            </div>
          </div>
        </div>
      {:else if flow === "share"}
        <div class="grid gap-4">
          <label class="grid gap-2 text-sm font-medium"
            >Board<Select.Root
              type="single"
              value={selected_board_id?.toString() ?? ""}
              onValueChange={(v) => {
                selected_board_id = v ? Number(v) : null;
                invalidate();
              }}
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
              onValueChange={(v) => {
                permission = v as "viewer" | "editor";
                invalidate();
              }}
              ><Select.Trigger class="w-full"
                >{permission === "viewer"
                  ? "Read only"
                  : "Can edit"}</Select.Trigger
              ><Select.Content
                ><Select.Item value="viewer">Read only</Select.Item><Select.Item
                  value="editor">Can edit</Select.Item
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
                    ? "Read only"
                    : "Can edit"}</Badge
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
                Create another invitation to manage access separately.</Card.Description
              ></Card.Root
            >
          {/if}
          <Button
            onclick={() => void create()}
            disabled={creating || selected_board_id === null}
            >{creating
              ? "Creating..."
              : generated
                ? "Create new invitation"
                : "Create invitation"}</Button
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
            oninput={() => (board.iroh_last_error = "")}
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
          role="group"
          aria-label="Sharing management views"
        >
          <Button
            bind:ref={manage_view_button}
            variant={manage_view === "mine" ? "secondary" : "outline"}
            aria-pressed={manage_view === "mine"}
            onclick={() => (manage_view = "mine")}
            >My invitations ({invites.length})</Button
          >
          <Button
            variant={manage_view === "received" ? "secondary" : "outline"}
            aria-pressed={manage_view === "received"}
            onclick={() => (manage_view = "received")}
            >Joined boards ({received_boards.length})</Button
          >
        </ButtonGroup.Root>
        <div
          class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
        >
          {#if manage_view === "mine"}
            <div class="flex items-center gap-2">
              <Checkbox id="pending-devices-only" bind:checked={pending_only} />
              <Label for="pending-devices-only" class="text-xs"
                >With pending device requests ({pending_count})</Label
              >
            </div>
          {:else}<span>Each board syncs with its owner.</span>{/if}
        </div>
        <p class="text-xs text-muted-foreground">
          {manage_view === "mine"
            ? "New devices need your approval before accessing a board. "
            : ""}This list updates automatically.
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
                <p>{manage_error}</p>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={loading_invites}
                  onclick={() => void load()}
                >
                  {loading_invites ? "Retrying..." : "Retry"}
                </Button>
              </div>
            {/if}
            {#if connection_details_error}
              <p class="text-xs text-muted-foreground" role="status">
                {connection_details_error}
              </p>
            {/if}
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
                          >{board.iroh_access_removed[shared.id]
                            ? (board.iroh_access_error[shared.id] ??
                              "Access is no longer available.")
                            : shared.sync_status === "syncing"
                              ? "Syncing with owner..."
                              : shared.sync_status === "conflict"
                                ? "Sync conflict: both versions are saved"
                                : shared.sync_status === "pending"
                                  ? "Local changes are waiting to sync"
                                  : shared.sync_status === "error"
                                    ? "Sync needs attention"
                                    : "Background sync enabled"}</Card.Description
                        >
                        <p class="mt-1 text-xs text-muted-foreground">
                          {board.iroh_last_synced_at[shared.id]
                            ? `Last synced this session: ${sync_time.format(new Date(board.iroh_last_synced_at[shared.id]))}`
                            : "No sync yet this session"}
                        </p>
                      </div>
                      <Badge variant="secondary"
                        >{shared.shared_role === "viewer"
                          ? "Read only"
                          : "Can edit"}</Badge
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
                        onclick={() => {
                          void board.switch_board(shared.id);
                          open = false;
                        }}>Open board</Button
                      >
                      {#if shared.sync_status === "conflict"}
                        <Button
                          variant="outline"
                          onclick={() =>
                            void board.resolve_iroh_conflict(shared.id, false)}
                          >Use owner version</Button
                        >
                        {#if shared.shared_role === "editor"}
                          <Button
                            variant="outline"
                            onclick={() =>
                              void board.resolve_iroh_conflict(shared.id, true)}
                            >Keep my version</Button
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
                        onclick={() => (received_waiting_id = shared.id)}
                        >Waiting for approval · View device ID</Button
                      >
                    {:else if board.iroh_access_removed[shared.id]}
                      <div
                        class="grid gap-2 rounded-lg border border-primary/25 bg-primary/5 p-3 text-sm"
                      >
                        <span
                          >{board.iroh_access_error[shared.id] ??
                            "Access is no longer available."} Your local board is
                          safe. Request access again; the owner must approve it. If
                          the invitation was deleted or disabled, ask the owner for
                          a new invitation or to enable sharing.</span
                        ><Button
                          class="w-full"
                          onclick={() =>
                            void request_received_access(shared.id, true)}
                          >Request access again</Button
                        >
                      </div>
                    {/if}
                    {@render connection_info(
                      connection_details.boards[shared.id],
                    )}
                    {#if board.iroh_sync_error[shared.id] && !board.iroh_access_removed[shared.id]}
                      <p class="text-sm text-destructive" role="status">
                        {board.iroh_sync_error[shared.id]}
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
                  >Share your first board</Card.Title
                >
                <Card.Description
                  >Create an invitation, send it to someone you trust, then
                  approve their device here.</Card.Description
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
                    {pending_count === 1 ? "access request" : "access requests"}
                    waiting for review</span
                  >
                </div>
              {/if}
              {#if visible_invites.length === 0}<p
                  class="rounded-md border p-4 text-sm text-muted-foreground"
                >
                  No devices are waiting for approval.
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
                      >{invite.enabled ? "Active" : "Disabled"}</Badge
                    >
                  </div>
                  <Collapsible.Root class="rounded-md border">
                    <Collapsible.Trigger
                      class="group flex w-full items-center justify-between gap-2 px-3 py-2 text-sm hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      <span
                        >Invitation settings · {invite.permission === "viewer"
                          ? "Read only"
                          : "Can edit"}</span
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
                            aria-label={`Access permission for ${invite.board_name}`}
                            >{invite.permission === "viewer"
                              ? "Read only"
                              : "Can edit"}</Select.Trigger
                          >
                          <Select.Content
                            ><Select.Item value="viewer">Read only</Select.Item
                            ><Select.Item value="editor">Can edit</Select.Item
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
                            >Sharing enabled</Label
                          >
                        </div>
                      </div>
                      <p class="text-xs text-muted-foreground">
                        Changes apply immediately. Turning sharing off stops
                        sync for every device using this invitation.
                      </p>
                    </Collapsible.Content>
                  </Collapsible.Root>
                  <div class="flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      disabled={!invite.enabled ||
                        loading_access_id === invite.invite_id}
                      onclick={() => void copy_invite(invite)}>Copy link</Button
                    >
                    <Button
                      variant="outline"
                      disabled={!invite.enabled ||
                        loading_access_id === invite.invite_id}
                      onclick={() => void toggle_qr(invite)}
                      >{visible_qr_id === invite.invite_id
                        ? "Hide QR"
                        : "Show QR"}</Button
                    >
                    <Button
                      class="ml-auto"
                      variant="destructive"
                      onclick={() => {
                        pending_delete = invite;
                        delete_confirm_open = true;
                      }}>Delete</Button
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
                          >Devices
                          <span class="text-muted-foreground"
                            >({invite.devices.length})</span
                          ></span
                        >
                        {#if invite.devices.some((device) => device.status === IrohDeviceStatus.Pending)}
                          <Badge variant="secondary"
                            >{invite.devices.filter(
                              (device) =>
                                device.status === IrohDeviceStatus.Pending,
                            ).length}
                            pending</Badge
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
                                      ? "Approval requested"
                                      : device.status ===
                                          IrohDeviceStatus.Approved
                                        ? "Approved device"
                                        : "Access revoked"}
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
                                    aria-haspopup="dialog">Review</Button
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
                                    }}>Revoke</Button
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
<AlertDialog.Root bind:open={discard_network_open}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>Discard unapplied changes?</AlertDialog.Title>
      <AlertDialog.Description
        >Your connection settings will keep their previously saved values.</AlertDialog.Description
      >
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>Keep editing</AlertDialog.Cancel>
      <AlertDialog.Action onclick={() => leave_network(true)}
        >Discard changes</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
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
        onclick={() => {
          pending_delete = null;
          delete_confirm_open = false;
        }}>Cancel</AlertDialog.Cancel
      ><AlertDialog.Action onclick={() => void remove()}
        >Delete invitation</AlertDialog.Action
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
      ><AlertDialog.Title>Review device request</AlertDialog.Title
      ><AlertDialog.Description
        >Confirm this ID with the person requesting access through a trusted
        channel. Approval grants this invitation's
        {approval_candidate?.invite.permission === "editor"
          ? "editing"
          : "viewing"}
        permission. Rejected devices may request again later.</AlertDialog.Description
      ></AlertDialog.Header
    >
    {#if approval_candidate}
      <div class="flex flex-wrap items-center gap-2 text-sm">
        <span class="font-medium">{approval_candidate.invite.board_name}</span>
        <Badge variant="secondary"
          >{approval_candidate.invite.permission === "viewer"
            ? "Read only"
            : "Can edit"}</Badge
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
          Device ID
        </div>
        <code class="block break-all text-sm leading-relaxed select-all"
          >{verification_id(approval_candidate.node_id)}</code
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
        >I compared the full device ID with the person requesting access.</Label
      >
    </div>
    <AlertDialog.Footer
      ><AlertDialog.Cancel
        class="disabled:opacity-100"
        disabled={device_action_pending}>Back</AlertDialog.Cancel
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
        }}>Reject</Button
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
        }}>Approve device</Button
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
      ><AlertDialog.Title>Revoke device access?</AlertDialog.Title
      ><AlertDialog.Description
        >This device will no longer be able to sync
        {revoke_candidate?.invite.board_name ?? "this board"}. Its local copy
        stays on the device, and it can request access again later.</AlertDialog.Description
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
        disabled={device_action_pending}>Cancel</AlertDialog.Cancel
      ><Button
        variant="destructive"
        class="disabled:opacity-100"
        disabled={device_action_pending}
        onclick={() => void revoke_device()}>Revoke access</Button
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
