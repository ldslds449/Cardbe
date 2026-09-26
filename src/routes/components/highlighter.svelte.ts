import { createHighlighterCoreSync } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import ts from "shiki/langs/typescript.mjs";
import github_dark from "shiki/themes/github-dark.mjs";
import github_light from "shiki/themes/github-light.mjs";
import { bundledLanguages, bundledLanguagesAlias } from "shiki/langs";

// This variable is private to this module and created ONLY ONCE
const highlighter = createHighlighterCoreSync({
  themes: [github_light, github_dark],
  langs: [ts],
  engine: createJavaScriptRegexEngine(),
});

export function getHighlighter() {
  return highlighter;
}

const language_loads = new Map<string, Promise<void>>();
const languages = { ...bundledLanguages, ...bundledLanguagesAlias };

export function loadHighlightLanguage(
  language: string,
): Promise<void> | undefined {
  if (highlighter.getLoadedLanguages().includes(language)) return;
  if (!Object.hasOwn(languages, language)) return;
  let pending = language_loads.get(language);
  if (!pending) {
    const loader = languages[language as keyof typeof languages];
    pending = highlighter.loadLanguage(loader);
    language_loads.set(language, pending);
    // A failed asset request can be retried the next time Markdown is rendered.
    void pending.catch(() => language_loads.delete(language));
  }
  return pending;
}
