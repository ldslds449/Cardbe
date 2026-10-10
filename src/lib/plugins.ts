import { translateCommandError, type CommandError } from "$lib/command-errors";
import { getLocale } from "$lib/i18n";
import { m } from "$lib/paraglide/messages.js";
import { formatNumber } from "$lib/i18n";

export type PluginText =
  | string
  | Partial<Record<"en" | "zh-TW", string>>
  | null;
export function pluginText(value: PluginText): string {
  return typeof value === "string"
    ? value
    : (value?.[getLocale()] ?? value?.en ?? "");
}

export interface PluginSetting {
  key: string;
  label: PluginText;
  type:
    | "text"
    | "number"
    | "boolean"
    | "select"
    | "secret"
    | "board"
    | "column";
  required: boolean;
  options?: { value: string; label: PluginText }[];
  min?: number | null;
  max?: number | null;
  default?: unknown;
}
export interface PluginPackage {
  id: string;
  name: PluginText;
  version: string;
  api_version: string;
  storage_schema_version: number;
  description: PluginText;
  domains: string[];
  settings: PluginSetting[];
  error_codes: string[];
  log_codes: string[];
}
export interface PluginInstance {
  id: string;
  plugin_id: string;
  name: string;
  board_id: number;
  config: Record<string, unknown>;
  secret_fields: string[];
  allowed_domains: string[];
  enabled: boolean;
  interval_seconds: number;
  last_run_at?: number;
  last_run_status?: PluginRun["status"] | null;
  next_run_at?: number;
  failures: number;
  last_error?: CommandError;
  running: boolean;
}
export interface PluginRun {
  id: string;
  instance_id: string;
  trigger: string;
  started_at: number;
  finished_at?: number;
  status: "running" | "success" | "failed" | "cancelled" | "interrupted";
  error?: CommandError;
  logs: { level: string; code: string; count: number }[];
  created: number;
  updated: number;
}
export interface PluginState {
  packages: PluginPackage[];
  instances: PluginInstance[];
  safe_mode: boolean;
}

export function pluginErrorMessage(error: unknown): string {
  return translateCommandError(error);
}
export function pluginLogMessage(log: PluginRun["logs"][number]): string {
  const count = formatNumber(log.count);
  return m.plugin_log_count({ code: log.code, count });
}

// Secrets remain separate from config and are never restored into form drafts.
export function pluginDefaults(pkg: PluginPackage): Record<string, unknown> {
  return Object.fromEntries(
    pkg.settings
      .filter((field) => field.type !== "secret" && field.default != null)
      .map((field) => [field.key, field.default]),
  );
}

export function invalidPluginSettings(
  fields: PluginSetting[],
  config: Record<string, unknown>,
  secrets: Record<string, string>,
  savedSecrets: string[],
): string[] {
  return fields
    .filter((field) => {
      const value =
        field.type === "secret" ? secrets[field.key] : config[field.key];
      const empty = value === undefined || value === null || value === "";
      if (empty) {
        return (
          field.required &&
          !(field.type === "secret" && savedSecrets.includes(field.key))
        );
      }
      if (field.type === "boolean") {
        return typeof value !== "boolean";
      }
      if (field.type === "number") {
        return (
          typeof value !== "number" ||
          !Number.isFinite(value) ||
          (field.min != null && value < field.min) ||
          (field.max != null && value > field.max)
        );
      }
      if (field.type === "board" || field.type === "column") {
        return (
          typeof value !== "number" || !Number.isSafeInteger(value) || value < 1
        );
      }
      if (field.type === "select") {
        return !field.options?.some((option) => option.value === value);
      }
      return typeof value !== "string" || (field.required && !value.trim());
    })
    .map((field) => field.key);
}
