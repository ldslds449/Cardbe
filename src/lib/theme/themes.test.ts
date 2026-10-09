import { describe, expect, it } from "vite-plus/test";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import {
  isThemePreference,
  restoreTheme,
  resolveTheme,
  themeAppearance,
  themes,
} from "./themes";
import { setLocale } from "$lib/paraglide/runtime";

const css = readFileSync("static/themes.css", "utf8");
const html = readFileSync("src/app.html", "utf8");
const bootstrap = html.match(/<script>([\s\S]*?)<\/script>/)![1];

function startup(
  cached: string | null,
  legacy: string | null,
  dark: boolean,
  storageFailure?: "read" | "write",
) {
  const storage = new Map<string, string>();
  if (cached != null) {
    storage.set("cardbe-theme", cached);
  }
  if (legacy != null) {
    storage.set("mode-watcher-mode", legacy);
  }
  const root = {
    dark: false,
    style: { colorScheme: "" },
    dataset: { theme: "" },
    classList: {
      toggle: (_: string, value: boolean) => {
        root.dark = value;
      },
    },
  };
  runInNewContext(bootstrap, {
    document: { documentElement: root },
    matchMedia: () => ({ matches: dark }),
    localStorage: {
      getItem: (key: string) => {
        if (storageFailure === "read") {
          throw new Error("Storage unavailable");
        }
        return storage.get(key) ?? null;
      },
      setItem: (key: string, value: string) => {
        if (storageFailure) {
          throw new Error("Storage unavailable");
        }
        storage.set(key, value);
      },
    },
  });
  return { root, storage };
}

function contrast(a: string, b: string) {
  const luminance = (hex: string) => {
    const channels = hex.match(/[\da-f]{2}/gi)!.map((channel) => {
      const value = parseInt(channel, 16) / 255;
      return value <= 0.04045
        ? value / 12.92
        : ((value + 0.055) / 1.055) ** 2.4;
    });
    return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  };
  const [low, high] = [luminance(a), luminance(b)].sort((x, y) => x - y);
  return (high + 0.05) / (low + 0.05);
}

function mixedColor(foreground: string, surface: string, percentage: number) {
  const channels = (hex: string) =>
    hex.match(/[\da-f]{2}/gi)!.map((value) => parseInt(value, 16));
  const first = channels(foreground);
  const second = channels(surface);
  return (
    "#" +
    first
      .map((value, index) =>
        Math.round(
          (value * percentage) / 100 + second[index] * (1 - percentage / 100),
        )
          .toString(16)
          .padStart(2, "0"),
      )
      .join("")
  );
}

describe("built-in themes", () => {
  it("registers ten themes with localized names and rejects arbitrary IDs", () => {
    expect(new Set(themes.map((theme) => theme.id)).size).toBe(10);
    for (const locale of ["en", "zh-TW"] as const) {
      setLocale(locale, { reload: false });
      for (const theme of themes) {
        expect(theme.name()).toBeTruthy();
      }
    }
    setLocale("en", { reload: false });
    expect(
      [undefined, null, {}, "custom", "LIGHT"].some(isThemePreference),
    ).toBe(false);
  });

  it.each(["light", "dark", "system"] as const)(
    "preserves legacy %s preference",
    (preference) => {
      expect(restoreTheme(null, preference)).toBe(preference);
      expect(restoreTheme(undefined, preference)).toBe(preference);
    },
  );

  it("uses system for corrupt IDs and respects explicit preferences", () => {
    expect(restoreTheme("corrupt", "dark")).toBe("system");
    expect(restoreTheme("morandi-sage", "dark")).toBe("morandi-sage");
    expect(restoreTheme(null, null)).toBe("system");
  });

  it.each(themes)(
    "restores $id before rendering and ignores OS changes for fixed themes",
    (theme) => {
      for (const dark of [false, true]) {
        const { root, storage } = startup(theme.id, "system", dark);
        expect(root.dataset.theme).toBe(theme.id);
        expect(root.dark).toBe(theme.appearance === "dark");
        expect(root.style.colorScheme).toBe(theme.appearance);
        expect(storage.get("mode-watcher-mode")).toBe(theme.appearance);
        expect(themeAppearance(theme.id)).toBe(theme.appearance);
        expect(resolveTheme(theme.id, dark ? "dark" : "light")).toBe(theme.id);
      }
    },
  );

  it("resolves System to classic light/dark and validates the startup cache", () => {
    for (const dark of [false, true]) {
      const appearance = dark ? "dark" : "light";
      expect(startup("system", "light", dark).root.dataset.theme).toBe(
        appearance,
      );
      expect(startup("invalid", "light", dark).root.dataset.theme).toBe(
        appearance,
      );
      expect(resolveTheme("system", appearance)).toBe(appearance);
    }
    expect(startup(null, "dark", false).root.dark).toBe(true);
  });

  it("keeps startup appearance consistent when storage reads or writes fail", () => {
    const fixed = startup("morandi-dark-blue", null, false, "write").root;
    expect(fixed.dataset.theme).toBe("morandi-dark-blue");
    expect(fixed.dark).toBe(true);
    expect(fixed.style.colorScheme).toBe("dark");
    for (const dark of [false, true]) {
      const fallback = startup("morandi-sage", null, dark, "read").root;
      expect(fallback.dataset.theme).toBe(dark ? "dark" : "light");
      expect(fallback.dark).toBe(dark);
      expect(fallback.style.colorScheme).toBe(dark ? "dark" : "light");
    }
  });

  it("defines complete Morandi surface tokens with AA text contrast", () => {
    for (const theme of themes.filter((theme) =>
      theme.id.startsWith("morandi-"),
    )) {
      const block = css
        .split(`.theme-preview[data-theme="${theme.id}"] {`)[1]
        .split("}")[0];
      const tokens = Object.fromEntries(
        [...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((match) => [
          match[1],
          match[2],
        ]),
      );
      const syntaxBlock =
        css.split(":root.dark {")[theme.appearance === "dark" ? 1 : 0];
      const weight = Number(
        css.match(/--code-syntax-weight: (\d+)%;\s*\}/)![1],
      );
      for (const match of syntaxBlock.matchAll(
        /--code-base-([\w-]+): (#[\da-f]{6});/g,
      )) {
        expect(
          contrast(mixedColor(match[2], tokens.primary, weight), tokens.card),
          `${theme.id}: syntax ${match[1]}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
      for (const token of [
        "background",
        "foreground",
        "card",
        "card-foreground",
        "popover",
        "popover-foreground",
        "primary",
        "primary-foreground",
        "secondary",
        "secondary-foreground",
        "muted",
        "muted-foreground",
        "accent",
        "accent-foreground",
        "border",
        "input",
        "ring",
        "sidebar",
        "sidebar-foreground",
        "sidebar-primary",
        "sidebar-primary-foreground",
        "sidebar-accent",
        "sidebar-accent-foreground",
        "sidebar-border",
        "sidebar-ring",
      ]) {
        expect(tokens[token], `${theme.id}: ${token}`).toBeTruthy();
      }
      expect(
        contrast(tokens.foreground, tokens.background),
      ).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tokens.foreground, tokens.card)).toBeGreaterThanOrEqual(
        4.5,
      );
      expect(
        contrast(tokens.foreground, tokens.sidebar),
      ).toBeGreaterThanOrEqual(4.5);
      expect(
        contrast(tokens.primary, tokens["primary-foreground"]),
      ).toBeGreaterThanOrEqual(4.5);
      expect(
        contrast(tokens.primary, tokens.background),
      ).toBeGreaterThanOrEqual(4.5);
      for (const surface of [tokens.background, tokens.card]) {
        expect(
          contrast(
            mixedColor(tokens.primary, surface, 80),
            tokens["primary-foreground"],
          ),
        ).toBeGreaterThanOrEqual(4.5);
      }
      const mutedPercentage = Number(
        tokens["muted-foreground"].match(/(\d+)%/)![1],
      );
      const inputPercentage = Number(tokens.input.match(/(\d+)%/)![1]);
      expect(
        contrast(
          mixedColor(tokens.foreground, tokens.card, mutedPercentage),
          tokens.secondary,
        ),
      ).toBeGreaterThanOrEqual(4.5);
      expect(
        contrast(
          mixedColor(tokens.foreground, tokens.card, inputPercentage),
          tokens.card,
        ),
      ).toBeGreaterThanOrEqual(3);
      expect(tokens.destructive).toBeUndefined();
      expect(tokens.success).toBeUndefined();
      expect(tokens.warning).toBeUndefined();
    }
  });

  it("keeps startup and backend allowlists in sync with the registry", () => {
    const backend = readFileSync("src-tauri/src/commands/settings.rs", "utf8");
    for (const theme of themes) {
      expect(bootstrap).toContain(`"${theme.id}"`);
      expect(backend).toContain(`"${theme.id}"`);
    }
  });
});
