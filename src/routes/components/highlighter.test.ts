import { describe, expect, it } from "vite-plus/test";
import rehypeShikiFromHighlighter from "@shikijs/rehype/core";
import { createParser } from "svelte-exmarkdown/utils";
import { getHighlighter, loadHighlightLanguage } from "./highlighter.svelte";

describe("Markdown code highlighting", () => {
  it.each([
    ["python", "    print('hello')"],
    ["js", "const answer = 42;"],
    ["json", '{ "answer": 42 }'],
    ["rust", "let answer = 42;"],
    ["csharp", "var answer = 42;"],
  ])("loads and highlights %s with theme variables", async (lang, code) => {
    await loadHighlightLanguage(lang);
    // Warm cold grammar regexes before checking the rendered tokens.
    getHighlighter().codeToTokensBase(code, {
      lang,
      theme: "cardbe",
      tokenizeTimeLimit: 0,
    });
    for (const theme of ["cardbe"]) {
      const html = getHighlighter().codeToHtml(code, { lang, theme });
      expect(html).toContain('<span style="color:var(--code-token-');
      expect(html).toContain("background-color:var(--code-background)");
      if (lang === "python") {
        expect(html).toContain("    ");
      }
      const parse = createParser([
        {
          rehypePlugin: [
            rehypeShikiFromHighlighter,
            getHighlighter(),
            { theme },
          ],
        },
      ]);
      const tree = JSON.stringify(parse(`\`\`\`${lang}\n${code}\n\`\`\``));
      expect(tree).toContain("color:");
      if (lang === "python") {
        expect(tree).toContain("    ");
      }
    }
  });

  it("shares concurrent grammar requests", async () => {
    const first = loadHighlightLanguage("ruby");
    expect(first).toBeDefined();
    expect(loadHighlightLanguage("ruby")).toBe(first);
    await first;
    expect(loadHighlightLanguage("ruby")).toBeUndefined();
  });

  it("leaves unknown languages readable without attempting a grammar load", () => {
    expect(loadHighlightLanguage("unknown-language")).toBeUndefined();
    expect(loadHighlightLanguage("__proto__")).toBeUndefined();
    const parse = createParser([
      {
        rehypePlugin: [
          rehypeShikiFromHighlighter,
          getHighlighter(),
          {
            theme: "cardbe",
            fallbackLanguage: "text",
          },
        ],
      },
    ]);
    const tree = parse("```unknown-language\nkeep this code\n```");
    expect(JSON.stringify(tree)).toContain("keep this code");
  });
});
