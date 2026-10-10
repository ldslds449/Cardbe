import { describe, expect, it } from "vite-plus/test";
import {
  invalidPluginSettings,
  pluginDefaults,
  pluginText,
  pluginErrorMessage,
  pluginLogMessage,
  type PluginPackage,
  type PluginSetting,
} from "./plugins";
import {
  applyLanguagePreference,
  formatNumber,
  getLocale,
  language,
} from "$lib/i18n";
import {
  getLocale as runtimeGetLocale,
  overwriteGetLocale,
} from "$lib/paraglide/runtime.js";
import en from "../../messages/en.json";
import zh from "../../messages/zh-TW.json";

const fields: PluginSetting[] = [
  { key: "repository", label: "Repository", type: "text", required: true },
  {
    key: "limit",
    label: "Limit",
    type: "number",
    required: true,
    min: 1,
    max: 10,
  },
  { key: "enabled", label: "Enabled", type: "boolean", required: true },
  { key: "column", label: "Column", type: "column", required: true },
  {
    key: "state",
    label: "State",
    type: "select",
    required: true,
    options: [{ value: "open", label: "Open" }],
  },
  { key: "token", label: "Token", type: "secret", required: true },
];
describe("plugin settings", () => {
  it("accepts column zero but rejects negative columns and board zero", () => {
    const settings: PluginSetting[] = [
      { key: "column", label: "Column", type: "column", required: true },
      { key: "board", label: "Board", type: "board", required: true },
    ];
    expect(
      invalidPluginSettings(settings, { column: 0, board: 1 }, {}, []),
    ).toEqual([]);
    expect(
      invalidPluginSettings(settings, { column: -1, board: 0 }, {}, []),
    ).toEqual(["column", "board"]);
  });
  it("keeps Taiwanese translations and switches generic diagnostics live", () => {
    expect(zh.plugin_title).toBe("外掛程式");
    for (const [key, value] of Object.entries(zh)) {
      if (key.startsWith("plugin_")) {
        expect(value).not.toMatch(/\?{2,}/);
      }
    }
    const originalGetLocale = runtimeGetLocale;
    const originalPreference = language.preference;
    overwriteGetLocale(getLocale);
    try {
      for (const locale of ["en", "zh-TW", "en"] as const) {
        applyLanguagePreference(locale);
        const catalog = locale === "en" ? en : zh;
        expect(pluginText({ en: "English", "zh-TW": "中文" })).toBe(
          locale === "en" ? "English" : "中文",
        );
        expect(pluginText(null)).toBe("");
        expect(pluginText({ en: "Fallback" })).toBe("Fallback");
        expect(
          pluginErrorMessage({
            code: "PLUGIN_ERROR",
            plugin_id: "fixture",
            plugin_error: { code: "RATE_LIMITED", params: {} },
          }),
        ).toBe(catalog.error_plugin_execution_failed);
        for (const count of [0, 1, 1234]) {
          expect(
            pluginLogMessage({
              level: "info",
              code: "ITEMS_SYNCED",
              count,
            }),
          ).toContain(formatNumber(count));
        }
        expect(
          pluginErrorMessage({
            code: "PLUGIN_ERROR",
            plugin_id: "untrusted",
            plugin_error: { code: "RATE_LIMITED", params: {} },
          }),
        ).toBe(catalog.error_plugin_execution_failed);
      }
    } finally {
      applyLanguagePreference(originalPreference);
      overwriteGetLocale(originalGetLocale);
    }
  });
  it("validates each schema type, bounds and required secrets", () => {
    const config = {
      repository: "owner/repo",
      limit: 5,
      enabled: false,
      column: 2,
      state: "open",
    };
    expect(invalidPluginSettings(fields, config, {}, ["token"])).toEqual([]);
    expect(
      invalidPluginSettings(fields, config, { token: "new-secret" }, []),
    ).toEqual([]);
    expect(
      invalidPluginSettings(
        fields,
        {
          repository: " ",
          limit: Infinity,
          enabled: "false",
          column: -1,
          state: "closed",
        },
        {},
        [],
      ),
    ).toEqual(fields.map((field) => field.key));
    expect(
      invalidPluginSettings(fields, { ...config, limit: 11 }, {}, ["token"]),
    ).toEqual(["limit"]);
  });
  it("does not put secret defaults or nullable defaults into config", () => {
    const pkg = {
      settings: [
        { key: "limit", type: "number", default: 5 },
        { key: "token", type: "secret", default: "never-copy" },
        { key: "missing", type: "text", default: null },
      ],
    } as PluginPackage;
    expect(pluginDefaults(pkg)).toEqual({ limit: 5 });
    expect(
      invalidPluginSettings(
        [
          {
            key: "limit",
            label: "Limit",
            type: "number",
            required: true,
            min: null,
            max: null,
          },
        ],
        { limit: -1 },
        {},
        [],
      ),
    ).toEqual([]);
  });
});
