export const supportedLocales = ["en", "zh-TW"] as const;
export type Locale = (typeof supportedLocales)[number];
export type LanguagePreference = "system" | Locale;

export function isLanguagePreference(
  value: unknown,
): value is LanguagePreference {
  return value === "system" || value === "en" || value === "zh-TW";
}

export function resolveLocale(
  preference: LanguagePreference = "system",
  systemLocale?: string,
): Locale {
  if (preference !== "system") {
    return preference;
  }
  const locale = systemLocale?.replaceAll("_", "-").toLowerCase();
  if (
    locale === "zh-tw" ||
    locale === "zh-hant" ||
    locale?.startsWith("zh-hant-")
  ) {
    return "zh-TW";
  }
  return "en";
}
