import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";
import { m } from "$lib/paraglide/messages";
import { applyLanguagePreference, getLocale } from "$lib/i18n";
import {
  getLocale as runtimeGetLocale,
  overwriteGetLocale,
} from "$lib/paraglide/runtime";
import { translateCommandError } from "$lib/command-errors";
import { client_configuration, valid_port } from "./mcp_server";

describe("MCP server settings", () => {
  const originalGetLocale = runtimeGetLocale;
  beforeEach(() => {
    applyLanguagePreference("en");
    overwriteGetLocale(getLocale);
  });
  afterEach(() => {
    applyLanguagePreference("system");
    overwriteGetLocale(originalGetLocale);
  });

  it("accepts the same ports as the backend", () => {
    for (const port of [1024, 8765, 65535]) {
      expect(valid_port(port)).toBe(true);
    }
    for (const port of [0, 80, 1023, 65536, 8765.5, Number.NaN]) {
      expect(valid_port(port)).toBe(false);
    }
  });

  it("builds an HTTP client configuration with the bearer token", () => {
    const parsed = JSON.parse(
      client_configuration("http://127.0.0.1:8765/mcp", "cardbe_mcp_secret"),
    );
    expect(parsed.mcpServers.cardbe).toEqual({
      type: "http",
      url: "http://127.0.0.1:8765/mcp",
      headers: { Authorization: "Bearer cardbe_mcp_secret" },
    });
  });

  it("localizes counts, states and errors in both languages", () => {
    for (const locale of ["en", "zh-TW"] as const) {
      applyLanguagePreference(locale);
      const counts = [0, 1, 2].map((count) =>
        m.mcp_client_board_count({ count }),
      );
      expect(new Set(counts).size).toBe(3);
      expect(m.mcp_status_failed()).not.toBe(m.mcp_status_stopped());
      expect(m.mcp_title()).toBe(locale === "en" ? "MCP server" : "MCP 伺服器");
      expect(translateCommandError({ code: "MCP_PORT_UNAVAILABLE" })).not.toBe(
        m.error_internal(),
      );
    }
    applyLanguagePreference("en");
    expect(m.mcp_client_board_count({ count: 1 })).toBe("1 board");
    expect(m.mcp_client_board_count({ count: 1200 })).toBe("1,200 boards");
    applyLanguagePreference("zh-TW");
    expect(m.mcp_client_board_count({ count: 2 })).toBe("2 個看板");
    expect(m.mcp_activity_duration({ ms: 1200 })).toBe("1,200 毫秒");
    expect(m.mcp_activity_removed_client()).toBe("已移除的應用程式");
    applyLanguagePreference("en");
    expect(m.mcp_activity_duration({ ms: 1200 })).toBe("1,200 ms");
  });

  it("keeps the settings scrollable only through the dialog's ScrollArea", () => {
    const source = readFileSync(
      new URL("./mcp_server_settings.svelte", import.meta.url),
      "utf8",
    );
    expect(source).not.toMatch(/overflow-(?:auto|scroll|x-|y-)/);
    const activity = readFileSync(
      new URL("./mcp_activity.svelte", import.meta.url),
      "utf8",
    );
    expect(activity).not.toMatch(/overflow-(?:auto|scroll|x-|y-)/);
  });
});
