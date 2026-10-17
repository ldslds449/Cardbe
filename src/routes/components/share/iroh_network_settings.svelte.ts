import { type CommandError, parseCommandError } from "$lib/command-errors";
import { logger } from "$lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { tick } from "svelte";

export type NetworkSettingsField =
  | "listen_port"
  | "direct_addresses"
  | "relay_urls"
  | "discovery_urls";
export type IrohNetworkSettings = {
  enabled: boolean;
  direct_ip_enabled: boolean;
  discovery_enabled: boolean;
  discovery_urls: string[];
  relay_enabled: boolean;
  relay_urls: string[];
  direct_addresses: string[];
  listen_port: number;
};

const FIELD_INPUT_IDS: Record<NetworkSettingsField, string> = {
  listen_port: "network-listen-port",
  direct_addresses: "network-direct-addresses",
  relay_urls: "network-relay-urls",
  discovery_urls: "network-discovery-urls",
};

function split_addresses(value: string) {
  return value
    .split(/[\n,]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

// Editable Iroh connection settings, kept separate from the sharing dialog
// so the dialog only decides when the form is shown or left.
export class IrohNetworkSettingsDraft {
  settings = $state<IrohNetworkSettings>({
    enabled: false,
    direct_ip_enabled: true,
    discovery_enabled: true,
    discovery_urls: [],
    relay_enabled: true,
    relay_urls: [],
    direct_addresses: [],
    listen_port: 0,
  });
  relay_urls_text = $state("");
  discovery_urls_text = $state("");
  direct_addresses_text = $state("");
  saving = $state(false);
  loading = $state(false);
  saved = $state("");
  applied = $state(false);
  restart_failed = $state(false);
  error = $state<unknown>(null);
  field_errors = $state<
    Partial<Record<NetworkSettingsField, CommandError | "port_range">>
  >({});
  advanced_open = $state(false);

  draft = $derived(
    JSON.stringify({
      enabled: this.settings.enabled,
      direct_ip_enabled: this.settings.direct_ip_enabled,
      discovery_enabled: this.settings.discovery_enabled,
      relay_enabled: this.settings.relay_enabled,
      relay_urls: split_addresses(this.relay_urls_text),
      discovery_urls: split_addresses(this.discovery_urls_text),
      direct_addresses: split_addresses(this.direct_addresses_text),
      listen_port: this.settings.listen_port ?? 0,
    }),
  );
  dirty = $derived(this.draft !== this.saved);
  has_transport = $derived(
    this.settings.direct_ip_enabled || this.settings.relay_enabled,
  );
  mode = $derived(
    this.settings.direct_ip_enabled && this.settings.relay_enabled
      ? "automatic"
      : this.settings.direct_ip_enabled && !this.settings.relay_enabled
        ? "direct"
        : !this.settings.direct_ip_enabled && this.settings.relay_enabled
          ? "relay"
          : "",
  );
  customized = $derived(
    Boolean(
      this.relay_urls_text.trim() ||
      this.discovery_urls_text.trim() ||
      this.direct_addresses_text.trim() ||
      this.settings.listen_port,
    ),
  );

  set_mode(mode: string) {
    if (!["automatic", "direct", "relay"].includes(mode)) {
      return;
    }
    this.settings.direct_ip_enabled = mode !== "relay";
    this.settings.relay_enabled = mode !== "direct";
    this.clear_error();
  }

  reset() {
    this.settings = {
      enabled: false,
      direct_ip_enabled: true,
      discovery_enabled: true,
      relay_enabled: true,
      direct_addresses: [],
      listen_port: 0,
      relay_urls: [],
      discovery_urls: [],
    };
    this.relay_urls_text =
      this.discovery_urls_text =
      this.direct_addresses_text =
        "";
    this.clear_error();
  }

  clear_error(field?: NetworkSettingsField) {
    if (field) {
      this.field_errors[field] = undefined;
    } else {
      this.field_errors = {};
    }
    this.error = null;
  }

  private async show_field(field: NetworkSettingsField) {
    this.advanced_open = true;
    await tick();
    const input = document.getElementById(FIELD_INPUT_IDS[field]);
    input?.focus();
    input?.scrollIntoView({ block: "nearest" });
  }

  async set_error(error: unknown) {
    this.clear_error();
    const parsed = parseCommandError(error);
    this.error = parsed ?? {
      code: "INTERNAL_ERROR",
    };
    const field =
      parsed?.code === "NETWORK_SETTINGS_INVALID" ? parsed.field : undefined;
    if (parsed?.code === "NETWORK_SETTINGS_INVALID" && field) {
      this.field_errors[field] = parsed;
      await this.show_field(field);
    }
  }

  /** Loads the saved settings into the form; errors are left to the caller. */
  async load() {
    this.clear_error();
    this.settings = await invoke<IrohNetworkSettings>(
      "get_iroh_network_settings",
    );
    this.relay_urls_text = this.settings.relay_urls.join("\n");
    this.discovery_urls_text = this.settings.discovery_urls.join("\n");
    this.direct_addresses_text = this.settings.direct_addresses.join("\n");
    this.saved = this.draft;
    this.applied = false;
    this.advanced_open = false;
  }

  /**
   * Validates and saves the draft. `on_saved` runs while the form is still
   * marked as saving, with the backend's restart error (null on success).
   */
  async save(
    on_saved: (
      settings: IrohNetworkSettings,
      restart_error: unknown,
    ) => Promise<void>,
  ) {
    if (
      this.saving ||
      !this.has_transport ||
      (!this.dirty && !this.restart_failed)
    ) {
      return;
    }
    const settings = JSON.parse(this.draft) as IrohNetworkSettings;
    if (
      !Number.isInteger(settings.listen_port) ||
      settings.listen_port < 0 ||
      settings.listen_port > 65535
    ) {
      this.clear_error();
      this.field_errors.listen_port = "port_range";
      this.advanced_open = true;
      await tick();
      document.getElementById(FIELD_INPUT_IDS.listen_port)?.focus();
      return;
    }
    this.saving = true;
    this.applied = false;
    this.clear_error();
    try {
      const restart_error = await invoke<unknown>("set_iroh_network_settings", {
        settings,
      });
      this.saved = JSON.stringify(settings);
      this.restart_failed = restart_error !== null;
      this.applied = restart_error === null;
      this.error = restart_error;
      await on_saved(settings, restart_error);
    } catch (e) {
      logger.error("iroh.network_settings_save.failed", e);
      await this.set_error(e);
    } finally {
      this.saving = false;
    }
  }
}
