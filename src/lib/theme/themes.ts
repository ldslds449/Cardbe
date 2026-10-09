import { m } from "$lib/paraglide/messages";
export const themes = [
  {
    id: "light",
    name: () => m.ui_light(),
    appearance: "light",
  },
  {
    id: "dark",
    name: () => m.ui_dark(),
    appearance: "dark",
  },
  {
    id: "morandi-sage",
    name: () => m.theme_morandi_sage(),
    appearance: "light",
  },
  {
    id: "morandi-rose",
    name: () => m.theme_morandi_rose(),
    appearance: "light",
  },
  {
    id: "morandi-blue",
    name: () => m.theme_morandi_blue(),
    appearance: "light",
  },
  {
    id: "morandi-sand",
    name: () => m.theme_morandi_sand(),
    appearance: "light",
  },
  {
    id: "morandi-dark-sage",
    name: () => m.theme_morandi_dark_sage(),
    appearance: "dark",
  },
  {
    id: "morandi-dark-blue",
    name: () => m.theme_morandi_dark_blue(),
    appearance: "dark",
  },
  {
    id: "morandi-dark-mauve",
    name: () => m.theme_morandi_dark_mauve(),
    appearance: "dark",
  },
  {
    id: "morandi-dark-cocoa",
    name: () => m.theme_morandi_dark_cocoa(),
    appearance: "dark",
  },
] as const;
export type ThemeId = (typeof themes)[number]["id"];
export type ThemePreference = ThemeId | "system";
export function isThemePreference(value: unknown): value is ThemePreference {
  return value === "system" || themes.some((theme) => theme.id === value);
}
export function resolveTheme(
  preference: ThemePreference,
  appearance: "light" | "dark",
): ThemeId {
  return preference === "system" ? appearance : preference;
}
export function themeAppearance(
  preference: ThemePreference,
): "light" | "dark" | "system" {
  return preference === "system"
    ? "system"
    : themes.find((theme) => theme.id === preference)!.appearance;
}
export function restoreTheme(
  stored: unknown,
  legacy: unknown,
): ThemePreference {
  if (isThemePreference(stored)) {
    return stored;
  }
  if (
    stored == null &&
    (legacy === "light" || legacy === "dark" || legacy === "system")
  ) {
    return legacy;
  }
  return "system";
}
