import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";
import { parseCommandError, translateCommandError } from "./command-errors";
import { applyLanguagePreference } from "$lib/i18n";
import { getLocale } from "$lib/i18n";
import {
  getLocale as runtimeGetLocale,
  overwriteGetLocale,
} from "$lib/paraglide/runtime";
import { m } from "$lib/paraglide/messages";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

describe("command error boundary", () => {
  const originalGetLocale = runtimeGetLocale;
  beforeEach(() => {
    applyLanguagePreference("en");
    overwriteGetLocale(getLocale);
  });
  afterEach(() => {
    applyLanguagePreference("system");
    overwriteGetLocale(originalGetLocale);
  });

  it("keeps every application command on the structured error contract", () => {
    const root = fileURLToPath(
      new URL("../../src-tauri/src/", import.meta.url),
    );
    const files = [
      "lib.rs",
      ...readdirSync(join(root, "commands"), { recursive: true })
        .filter((file) => file.toString().endsWith(".rs"))
        .map((file) => join("commands", file.toString())),
    ];
    let commands = 0;
    for (const file of files) {
      const source = readFileSync(join(root, file), "utf8");
      for (const command of source.split("#[tauri::command]").slice(1)) {
        const signature = command.slice(0, command.indexOf("{"));
        commands++;
        if (signature.includes("-> Result<")) {
          expect(signature.trim(), file).toMatch(/, (?:\w+::)*CommandError>$/);
        }
      }
    }
    expect(commands).toBeGreaterThan(70);
  });

  it("recognizes and localizes every Rust error code in both languages", () => {
    const source = readFileSync(
      new URL("../../src-tauri/src/errors.rs", import.meta.url),
      "utf8",
    ).replaceAll("\r\n", "\n");
    const contract = source.split("pub enum CommandError {")[1].split("\n}")[0];
    const variants = [...contract.matchAll(/^    ([A-Z]\w+)(?:,| \{)/gm)];
    expect(variants.length).toBeGreaterThan(35);
    for (const [, variant] of variants) {
      const code = variant
        .replace(/[A-Z]/g, (letter, index) =>
          index === 0 ? letter : `_${letter}`,
        )
        .toUpperCase();
      const error = {
        code,
        field: "direct_addresses",
        device_id: "device",
        plugin_id: "plugin",
        permission: "network",
        plugin_error: { code: "FAILED", params: {} },
        detail: "private database path and token",
      };
      expect(parseCommandError(error), code).not.toBeNull();
      for (const locale of ["en", "zh-TW"] as const) {
        applyLanguagePreference(locale);
        const message = translateCommandError(error);
        expect(message, code).not.toContain(error.detail);
        if (code !== "INTERNAL_ERROR") {
          expect(message, code).not.toBe(m.error_internal());
        }
      }
    }
  });

  it("preserves valid network fields and localizes them after language changes", () => {
    for (const field of [
      "listen_port",
      "direct_addresses",
      "relay_urls",
      "discovery_urls",
    ]) {
      const error = { code: "NETWORK_SETTINGS_INVALID", field };
      expect(parseCommandError({ ...error, detail: "secret" })).toEqual(error);
      applyLanguagePreference("en");
      const english = translateCommandError(error);
      applyLanguagePreference("zh-TW");
      expect(translateCommandError(error)).not.toBe(english);
      expect(translateCommandError(error)).not.toBe(m.error_internal());
    }
    for (const field of [undefined, "unknown", {}, ["relay_urls"]]) {
      expect(
        parseCommandError({ code: "NETWORK_SETTINGS_INVALID", field }),
      ).toBeNull();
    }
  });

  it("preserves structured sharing state without parsing backend wording", () => {
    expect(
      parseCommandError({
        code: "SHARE_APPROVAL_REQUIRED",
        device_id: "device",
      }),
    ).toEqual({ code: "SHARE_APPROVAL_REQUIRED", device_id: "device" });
    expect(parseCommandError({ code: "SHARE_APPROVAL_REQUIRED" })).toBeNull();
    expect(translateCommandError({ code: "SHARE_ACCESS_REVOKED" })).toBe(
      m.ui_access_was_declined_or_revoked(),
    );
    expect(translateCommandError("Access was declined or revoked")).toBe(
      m.error_internal(),
    );
  });
  it("validates unknown IPC values and strips extra internal details", () => {
    expect(
      parseCommandError({ code: "TASK_NOT_FOUND", secret: "private" }),
    ).toEqual({ code: "TASK_NOT_FOUND" });
    expect(
      parseCommandError({
        code: "PLUGIN_PERMISSION_DENIED",
        plugin_id: "github-sync",
        permission: "network",
      }),
    ).toEqual({
      code: "PLUGIN_PERMISSION_DENIED",
      plugin_id: "github-sync",
      permission: "network",
    });
    for (const error of [
      null,
      "SQLite password=secret",
      new Error("private"),
      [],
      { code: "NEW_ERROR" },
      { code: "PLUGIN_PERMISSION_DENIED", plugin_id: "github-sync" },
    ]) {
      expect(parseCommandError(error)).toBeNull();
      expect(translateCommandError(error)).toBe(m.error_internal());
    }
  });

  it("localizes core errors in both languages without caching messages", () => {
    applyLanguagePreference("en");
    expect(translateCommandError({ code: "TASK_NOT_FOUND" })).toBe(
      "This task no longer exists.",
    );
    applyLanguagePreference("zh-TW");
    expect(translateCommandError({ code: "TASK_NOT_FOUND" })).toBe(
      "這項任務已不存在。",
    );
    applyLanguagePreference("en");
  });

  it("explains that a saved conflict needs resolution in the current language", () => {
    const error = { code: "SHARE_SYNC_CONFLICT" };
    for (const [locale, message] of [
      ["en", "Resolve the saved sync conflict before syncing again."],
      ["zh-TW", "請先處理已儲存的同步衝突，再重新同步。"],
      ["en", "Resolve the saved sync conflict before syncing again."],
    ] as const) {
      applyLanguagePreference(locale);
      expect(translateCommandError(error)).toBe(message);
    }
  });

  it("isolates plugin resources and falls back for unknown or broken translations", () => {
    const error = {
      code: "PLUGIN_ERROR",
      plugin_id: "github-sync",
      plugin_error: {
        code: "REPOSITORY_NOT_FOUND",
        params: { repository: "owner/repo" },
      },
    };
    const resources = {
      "github-sync": {
        en: {
          REPOSITORY_NOT_FOUND: (params: Record<string, unknown>) =>
            `Repository ${params.repository} was not found.`,
        },
        "zh-TW": {
          REPOSITORY_NOT_FOUND: (params: Record<string, unknown>) =>
            `找不到儲存庫 ${params.repository}。`,
        },
      },
    };
    applyLanguagePreference("en");
    expect(translateCommandError(error, resources)).toBe(
      "Repository owner/repo was not found.",
    );
    applyLanguagePreference("zh-TW");
    expect(translateCommandError(error, resources)).toBe(
      "找不到儲存庫 owner/repo。",
    );
    expect(translateCommandError(error)).toBe(
      m.error_plugin_execution_failed(),
    );
    expect(
      translateCommandError(
        { ...error, plugin_id: "another-plugin" },
        resources,
      ),
    ).toBe(m.error_plugin_execution_failed());
    expect(translateCommandError({ code: "TASK_NOT_FOUND" }, resources)).toBe(
      m.error_task_not_found(),
    );
    expect(
      translateCommandError(error, {
        "github-sync": {
          "zh-TW": {
            REPOSITORY_NOT_FOUND: () => {
              throw new Error("private");
            },
          },
        },
      }),
    ).toBe(m.error_plugin_execution_failed());
    applyLanguagePreference("en");
  });

  it("rejects nested, oversized, and non-finite plugin params", () => {
    for (const params of [
      { nested: {} },
      { value: "x".repeat(4096) },
      { value: Infinity },
    ]) {
      expect(
        parseCommandError({
          code: "PLUGIN_ERROR",
          plugin_id: "test",
          plugin_error: { code: "FAILED", params },
        }),
      ).toBeNull();
    }
  });
});
