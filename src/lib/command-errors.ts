import { m } from "$lib/paraglide/messages";
import { getLocale } from "$lib/i18n";

const coreMessages = {
  SHARE_UNAVAILABLE: () => m.ui_share_unavailable(),
  NOTE_NOT_FOUND: () => m.error_note_not_found(),
  NOTE_CONTENT_REQUIRED: () => m.error_note_content_required(),
  TEMPLATE_NOT_FOUND: () => m.error_template_not_found(),
  TEMPLATE_NAME_REQUIRED: () => m.error_template_name_required(),
  TASK_TITLE_REQUIRED: () => m.error_task_title_required(),
  INVALID_ARGUMENT: () => m.error_invalid_argument(),
  INVALID_IMPORT: () => m.error_invalid_import(),
  UNSUPPORTED_BACKUP_VERSION: () => m.error_unsupported_backup_version(),
  BOARD_NAME_INVALID: () => m.error_board_name_invalid(),
  INVITE_NOT_FOUND: () => m.error_invite_not_found(),
  INVITE_DISABLED: () => m.error_invite_disabled(),
  INVITE_INVALID: () => m.error_invite_invalid(),
  INVITE_ALREADY_JOINED: () => m.error_invite_already_joined(),
  DEVICE_REQUEST_NOT_FOUND: () => m.error_device_request_not_found(),
  SYNC_CONFLICT_NOT_FOUND: () => m.error_sync_conflict_not_found(),
  LAN_UNAVAILABLE: () => m.error_lan_unavailable(),
  SHARE_LINK_DISABLED: () => m.error_share_link_disabled(),
  SHARE_SNAPSHOT_INVALID: () => m.error_share_snapshot_invalid(),
  SHARE_LIMIT_EXCEEDED: () => m.error_share_limit_exceeded(),
  SHARE_EXPIRATION_INVALID: () => m.error_share_expiration_invalid(),
  CALENDAR_FONT_NOT_FOUND: () => m.error_calendar_font_not_found(),
  EXTERNAL_URL_INVALID: () => m.error_external_url_invalid(),
  TASK_NOT_FOUND: () => m.error_task_not_found(),
  BOARD_NOT_FOUND: () => m.error_board_not_found(),
  COLUMN_NOT_FOUND: () => m.error_column_not_found(),
  BOARD_NAME_REQUIRED: () => m.error_board_name_required(),
  LAST_BOARD_REQUIRED: () => m.error_last_board_required(),
  STALE_BOARD_REQUEST: () => m.error_stale_board_request(),
  PERMISSION_DENIED: () => m.error_permission_denied(),
  NOTHING_TO_UNDO: () => m.error_nothing_to_undo(),
  UPDATE_REQUEST_FAILED: () => m.update_network_error(),
  UPDATE_RELEASE_DATA_INVALID: () => m.update_release_data_error(),
  SHARE_ACCESS_REVOKED: () => m.ui_access_was_declined_or_revoked(),
  SHARE_ACCESS_TERMINATED: () => m.share_access_terminated(),
  SHARE_INVITATION_DELETED: () => m.share_invitation_deleted_error(),
  SHARE_SYNC_CONFLICT: () => m.share_conflict_error(),
  SHARE_FAILED: () => m.share_sync_error(),
  INTERNAL_ERROR: () => m.error_internal(),
};

export type NetworkSettingsField =
  | "listen_port"
  | "direct_addresses"
  | "relay_urls"
  | "discovery_urls";

type PluginParams = Record<string, string | number | boolean | null>;
export type CommandError =
  | { code: keyof typeof coreMessages }
  | { code: "NETWORK_SETTINGS_INVALID"; field: NetworkSettingsField }
  | { code: "SHARE_APPROVAL_REQUIRED"; device_id: string }
  | { code: "PLUGIN_PERMISSION_DENIED"; plugin_id: string; permission: string }
  | { code: "PLUGIN_EXECUTION_FAILED"; plugin_id: string }
  | {
      code: "PLUGIN_ERROR";
      plugin_id: string;
      plugin_error: { code: string; params: PluginParams };
    };

// A plugin can only supply messages within its own ID and locale. These
// functions must come from trusted host resources, never executable IPC data.
export type PluginErrorTranslations = Record<
  string,
  Partial<
    Record<"en" | "zh-TW", Record<string, (params: PluginParams) => string>>
  >
>;

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function identifier(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 128;
}

export function parseCommandError(value: unknown): CommandError | null {
  try {
    if (!record(value) || typeof value.code !== "string") {
      return null;
    }
    const code = value.code;
    if (Object.hasOwn(coreMessages, code)) {
      return { code: code as keyof typeof coreMessages };
    }
    if (code === "SHARE_APPROVAL_REQUIRED" && identifier(value.device_id)) {
      return { code, device_id: value.device_id };
    }
    if (code === "NETWORK_SETTINGS_INVALID") {
      if (
        [
          "listen_port",
          "direct_addresses",
          "relay_urls",
          "discovery_urls",
        ].includes(value.field as string)
      ) {
        return { code, field: value.field as NetworkSettingsField };
      }
      return null;
    }
    if (!identifier(value.plugin_id)) {
      return null;
    }
    const plugin_id = value.plugin_id;
    if (code === "PLUGIN_EXECUTION_FAILED") {
      return { code, plugin_id };
    }
    if (code === "PLUGIN_PERMISSION_DENIED" && identifier(value.permission)) {
      return { code, plugin_id, permission: value.permission };
    }
    if (code === "PLUGIN_ERROR" && record(value.plugin_error)) {
      const error = value.plugin_error;
      const params = error.params ?? {};
      if (
        typeof error.code !== "string" ||
        !/^[A-Z0-9_]{1,64}$/.test(error.code) ||
        !record(params) ||
        Object.keys(params).length > 16 ||
        !Object.values(params).every(
          (param) =>
            param === null ||
            typeof param === "string" ||
            typeof param === "boolean" ||
            (typeof param === "number" && Number.isFinite(param)),
        ) ||
        new TextEncoder().encode(JSON.stringify({ code: error.code, params }))
          .length > 4096
      ) {
        return null;
      }
      return {
        code,
        plugin_id,
        plugin_error: {
          code: error.code,
          params: { ...params } as PluginParams,
        },
      };
    }
  } catch {
    // Even unusual thrown objects must produce a safe localized fallback.
  }
  return null;
}

/** Presentation stays with the caller (toast, inline error, dialog, etc.). */
export function translateCommandError(
  value: unknown,
  translations?: PluginErrorTranslations,
): string {
  const error = parseCommandError(value);
  if (!error) {
    return m.error_internal();
  }
  switch (error.code) {
    case "NETWORK_SETTINGS_INVALID":
      switch (error.field) {
        case "listen_port":
          return m.share_network_fixed_port_error();
        case "direct_addresses":
          return m.share_network_direct_addresses_error();
        case "relay_urls":
          return m.share_network_relay_urls_error();
        case "discovery_urls":
          return m.share_network_discovery_urls_error();
      }
    case "SHARE_APPROVAL_REQUIRED":
      return m.ui_access_request_sent();
    case "PLUGIN_PERMISSION_DENIED":
      return m.error_plugin_permission_denied();
    case "PLUGIN_EXECUTION_FAILED":
      return m.error_plugin_execution_failed();
    case "PLUGIN_ERROR": {
      const resources =
        translations && Object.hasOwn(translations, error.plugin_id)
          ? translations[error.plugin_id]?.[getLocale()]
          : undefined;
      const message =
        resources && Object.hasOwn(resources, error.plugin_error.code)
          ? resources[error.plugin_error.code]
          : undefined;
      try {
        const result = message?.(error.plugin_error.params);
        if (typeof result === "string" && result.trim()) {
          return result;
        }
      } catch {
        // Missing params or incompatible plugin translations use host fallback.
      }
      return m.error_plugin_execution_failed();
    }
    default:
      return coreMessages[error.code]();
  }
}
