import {
  parseCommandError,
  translateCommandError,
  type CommandError,
} from "$lib/command-errors";
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
  description?: PluginText;
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
  manifest_version?: number;
  api_version: string;
  storage_schema_version: number;
  description: PluginText;
  domains: string[];
  allow_custom_domains?: boolean;
  http_credentials?: HttpCredentialRule[] | null;
  settings: PluginSetting[];
  error_codes: string[];
  log_codes: string[];
}
export interface HttpCredentialRule {
  destination:
    | { type: "domain"; domain: string }
    | { type: "base_url"; setting: string };
  path_prefix: string;
  bearer_secret?: string | null;
  auth?:
    | { type: "bearer"; secret: string }
    | { type: "header"; name: string; secret: string }
    | null;
}
export function validPluginManifestVersion(pkg: PluginPackage): boolean {
  const version = pkg.manifest_version === undefined ? 1 : pkg.manifest_version;
  return (
    (version === 1 || version === 2) &&
    (version === 2 || !pkg.http_credentials?.some((rule) => rule.auth != null))
  );
}

function validCredentialHeader(name: string): boolean {
  return (
    name.length <= 128 &&
    /^[!#$%&'*+.^_`|~0-9a-z-]+$/i.test(name) &&
    !name.toLowerCase().startsWith("sec-") &&
    ![
      "authorization",
      "host",
      "cookie",
      "cookie2",
      "content-length",
      "connection",
      "keep-alive",
      "proxy-authorization",
      "proxy-authenticate",
      "proxy-connection",
      "te",
      "trailer",
      "transfer-encoding",
      "upgrade",
      "accept",
      "content-type",
      "user-agent",
    ].includes(name.toLowerCase())
  );
}

function validCredentialPath(path: string): boolean {
  return (
    path.length <= 2048 &&
    /^\/[a-zA-Z0-9/._~-]*$/.test(path) &&
    (path === "/" || !path.endsWith("/")) &&
    !path.includes("//") &&
    !path.split("/").some((part) => part === "." || part === "..")
  );
}

function credentialBaseUrl(value: unknown): URL | null {
  if (
    typeof value !== "string" ||
    value.length > 8192 ||
    /[?#\\]/.test(value)
  ) {
    return null;
  }
  // Check the original path before URL parsing removes dot segments.
  const match = /^https:\/\/[^/]+(\/.*)?$/i.exec(value);
  if (!match) {
    return null;
  }
  const rawPath = match[1] ?? "/";
  if (
    rawPath.includes("//") ||
    !validCredentialPath(rawPath.replace(/\/$/, "") || "/")
  ) {
    return null;
  }
  try {
    const url = new URL(value);
    const host = url.hostname;
    const path = url.pathname.replace(/\/+$/, "") || "/";
    if (
      url.protocol !== "https:" ||
      url.port === "0" ||
      url.username ||
      url.password ||
      url.search ||
      !validCredentialPath(path) ||
      host.length > 253 ||
      !host.includes(".") ||
      host === "localhost" ||
      host.endsWith(".localhost") ||
      !host
        .split(".")
        .every((label) => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(label))
    ) {
      return null;
    }
    // Backend additionally validates public addresses and pins DNS before HTTP.
    if (/^\d+\.\d+\.\d+\.\d+$/.test(host)) {
      const [a, b, c, d] = host.split(".").map(Number);
      if (
        a === 0 ||
        a === 10 ||
        a === 127 ||
        a >= 224 ||
        (a === 169 && b === 254) ||
        (a === 172 && b >= 16 && b <= 31) ||
        (a === 192 && b === 168) ||
        (a === 100 && b >= 64 && b <= 127) ||
        (a === 192 && b === 0 && c === 0) ||
        (a === 198 && (b === 18 || b === 19)) ||
        (a === 192 && b === 0 && c === 2) ||
        (a === 198 && b === 51 && c === 100) ||
        (a === 203 && b === 0 && c === 113) ||
        (a === 255 && b === 255 && c === 255 && d === 255)
      ) {
        return null;
      }
    }
    return url;
  } catch {
    return null;
  }
}

export function pluginCredentialScopes(
  pkg: PluginPackage,
  config: Record<string, unknown>,
) {
  return (pkg.http_credentials ?? []).flatMap((rule) => {
    const url =
      rule.destination.type === "base_url"
        ? credentialBaseUrl(config[rule.destination.setting])
        : null;
    if (rule.destination.type === "base_url" && !url) {
      return [];
    }
    const domain =
      rule.destination.type === "domain"
        ? canonicalPluginDomain(rule.destination.domain)
        : url!.host;
    const base = url?.pathname.replace(/\/+$/, "") ?? "";
    return [
      {
        domain,
        path: rule.path_prefix === "/" ? base || "/" : base + rule.path_prefix,
        secret: rule.auth?.secret ?? rule.bearer_secret ?? null,
        header:
          rule.auth?.type === "header"
            ? rule.auth.name
            : "Authorization: Bearer",
        setting:
          rule.destination.type === "base_url"
            ? rule.destination.setting
            : null,
      },
    ];
  });
}

export function invalidPluginCredentialSettings(
  pkg: PluginPackage,
  config: Record<string, unknown>,
): string[] {
  const invalid = new Set<string>();
  if (!validPluginManifestVersion(pkg)) {
    invalid.add("http_credentials");
  }
  for (const rule of pkg.http_credentials ?? []) {
    const secret = rule.auth?.secret ?? rule.bearer_secret;
    if (
      ("auth" in rule && "bearer_secret" in rule) ||
      (rule.auth?.type === "header" &&
        !validCredentialHeader(rule.auth.name)) ||
      (secret != null &&
        !pkg.settings.some(
          (field) => field.key === secret && field.type === "secret",
        ))
    ) {
      invalid.add(
        secret ??
          (rule.destination.type === "base_url"
            ? rule.destination.setting
            : "http_credentials"),
      );
    }
    if (rule.destination.type !== "base_url") {
      continue;
    }
    const key = rule.destination.setting;
    const value = config[key];
    if (value == null || value === "") {
      continue;
    }
    const url = credentialBaseUrl(value);
    if (
      !url ||
      (!pkg.allow_custom_domains &&
        !pkg.domains.some(
          (domain) => canonicalPluginDomain(domain) === url.host,
        ))
    ) {
      invalid.add(key);
    }
  }
  const scopes = pluginCredentialScopes(pkg, config);
  for (const [index, scope] of scopes.entries()) {
    for (const other of scopes.slice(0, index)) {
      if (scope.domain === other.domain && scope.path === other.path) {
        if (scope.setting) {
          invalid.add(scope.setting);
        }
        if (other.setting) {
          invalid.add(other.setting);
        }
      }
    }
  }
  return [...invalid];
}
export interface PluginInstance {
  id: string;
  plugin_id: string;
  name: string;
  board_id: number;
  config: Record<string, unknown>;
  secret_fields: string[];
  allowed_domains: string[];
  pending_domain?: { domain: string; run_id: string; trigger: string } | null;
  enabled: boolean;
  needs_review?: boolean;
  credentials_need_review?: boolean;
  interval_seconds: number;
  last_run_at?: number;
  last_run_status?: PluginRun["status"] | null;
  next_run_at?: number;
  failures: number;
  last_error?: CommandError;
  running: boolean;
}
export interface PluginUpdatePreview {
  token: string;
  current_version: string;
  package: PluginPackage;
  requires_review: boolean;
}
export type PluginPackageSelection =
  | { kind: "installed"; package: PluginPackage }
  | {
      kind: "update";
      current_package: PluginPackage;
      preview: PluginUpdatePreview;
    };
export function isNewerPluginVersion(current: string, next: string): boolean {
  const parse = (value: string) => {
    if (
      value.length > 62 ||
      !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value)
    ) {
      return null;
    }
    const parts = value.split(".").map(BigInt);
    return parts.every((part) => part <= 18446744073709551615n) ? parts : null;
  };
  const oldVersion = parse(current),
    newVersion = parse(next);
  if (!oldVersion || !newVersion) {
    return false;
  }
  for (let index = 0; index < 3; index++) {
    if (newVersion[index] !== oldVersion[index]) {
      return newVersion[index] > oldVersion[index];
    }
  }
  return false;
}

export interface PluginRun {
  id: string;
  instance_id: string;
  trigger: string;
  started_at: number;
  finished_at?: number;
  status:
    | "running"
    | "success"
    | "failed"
    | "cancelled"
    | "interrupted"
    | "awaiting_permission";
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
  const parsed = parseCommandError(error);
  const message = translateCommandError(error);
  return parsed?.code === "PLUGIN_ERROR"
    ? m.plugin_error_details({ message, code: parsed.plugin_error.code })
    : message;
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
          typeof value !== "number" ||
          !Number.isSafeInteger(value) ||
          value < (field.type === "column" ? 0 : 1)
        );
      }
      if (field.type === "select") {
        return !field.options?.some((option) => option.value === value);
      }
      if (field.type === "secret" && typeof value === "string") {
        // Match the backend HTTP header-value check: visible ASCII, spaces and tabs only.
        return (
          !/^[\t\x20-\x7e]*$/.test(value) ||
          value.length > 8192 ||
          (field.required && !value.trim())
        );
      }
      return typeof value !== "string" || (field.required && !value.trim());
    })
    .map((field) => field.key);
}

export function canonicalPluginDomain(domain: string): string {
  return domain.endsWith(":443") ? domain.slice(0, -4) : domain;
}
