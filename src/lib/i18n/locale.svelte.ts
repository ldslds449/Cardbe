import { browser } from "$app/environment";
import {
  defineCustomClientStrategy,
  overwriteGetLocale,
} from "$lib/paraglide/runtime.js";
import {
  isLanguagePreference,
  resolveLocale,
  type LanguagePreference,
} from "./locale";

// The preference is the only mutable locale state. Message calls read it through
// Paraglide, so Svelte tracks them without remounting or losing unsaved forms.
export const language = $state({ preference: "system" as LanguagePreference });
let systemLocale = $state(browser ? navigator.language : undefined);
export function getLocale() {
  return resolveLocale(language.preference, systemLocale);
}
export function applyLanguagePreference(value: unknown) {
  language.preference = isLanguagePreference(value) ? value : "system";
}
export function refreshSystemLocale() {
  if (browser) {
    systemLocale = navigator.language;
  }
}
if (browser) {
  defineCustomClientStrategy("custom-cardbe", {
    getLocale,
    setLocale: applyLanguagePreference,
  });
  // CSR-only: bypass the runtime cache to preserve Svelte dependency tracking.
  overwriteGetLocale(getLocale);
}
