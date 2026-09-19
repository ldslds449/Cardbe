import { describe, expect, it } from "vitest";
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
  ])("loads and highlights %s in both themes", async (lang, code) => {
    await loadHighlightLanguage(lang);
    for (const theme of ["github-light", "github-dark"]) {
      const html = getHighlighter().codeToHtml(code, { lang, theme });
      expect(html).toContain('<span style="color:');
      if (lang === "python") expect(html).toContain('    ');
      const parse = createParser([{
        rehypePlugin: [rehypeShikiFromHighlighter, getHighlighter(), { theme }],
      }]);
      const tree = JSON.stringify(parse(`\`\`\`${lang}\n${code}\n\`\`\``));
      expect(tree).toContain("color:");
      if (lang === "python") expect(tree).toContain('    ');
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
    const parse = createParser([{
      rehypePlugin: [rehypeShikiFromHighlighter, getHighlighter(), {
        theme: "github-dark", fallbackLanguage: "text",
      }],
    }]);
    const tree = parse("```unknown-language\nkeep this code\n```");
    expect(JSON.stringify(tree)).toContain("keep this code");
  });
});
