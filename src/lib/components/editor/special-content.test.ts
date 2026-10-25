import { describe, expect, it } from "vite-plus/test";
import { createParser } from "svelte-exmarkdown/utils";
import type { HastNode } from "svelte-exmarkdown";
import remarkMath from "remark-math";
import { markdownSpecial } from "./markdown-special";
import { renderMath } from "./math";
import { renderDiagram } from "./mermaid";

const parse = (source: string) =>
  createParser([
    { remarkPlugin: remarkMath, rehypePlugin: [markdownSpecial, source] },
  ])(source);
function formulas(tree: HastNode): HastNode[] {
  return [tree, ...(tree.children?.flatMap(formulas) ?? [])].filter(
    (node) =>
      node.type === "element" &&
      ["math-formula", "mermaid-diagram"].includes(node.tagName),
  );
}

describe("Markdown formulas and diagrams", () => {
  it("renders double-dollar formulas as display math, even on one line", () => {
    expect(
      formulas(parse("Before $$x^2$$ after $y$") as HastNode).map(
        (node) => node.properties,
      ),
    ).toEqual([
      { source: "x^2", display: true },
      { source: "y", display: false },
    ]);
    expect(formulas(parse("$$x^2$$") as HastNode)[0].properties?.display).toBe(
      true,
    );
    expect(formulas(parse("$ x^2 $") as HastNode)[0].properties?.display).toBe(
      false,
    );
  });
  it("parses inline and multiline formulas without touching code or escaped dollars", () => {
    const tree = parse(
      "公式 $x^2$ 😀\n\n$$\n\\frac{1}{2}\n$$\n\n`$literal$` and \\$escaped\\$\n\n```text\n$x$\n```",
    ) as HastNode;
    expect(formulas(tree).map((node) => node.properties)).toEqual([
      { source: "x^2", display: false },
      { source: "\\frac{1}{2}", display: true },
    ]);
  });

  it("passes Mermaid source intact to its renderer", () => {
    const source = "flowchart LR\n  A[開始] --> B[完成]";
    expect(
      formulas(parse(`\`\`\`mermaid\n${source}\n\`\`\``) as HastNode)[0]
        .properties?.source,
    ).toBe(source);
  });

  it("renders KaTeX HTML and accessible MathML with bounded, untrusted input", () => {
    expect(renderMath("x^2")).toContain('class="katex"');
    expect(renderMath("x^2")).toContain("<math");
    expect(renderMath("x^2", true)).toContain("katex-display");
    expect(renderMath("\\frac{")).toBeNull();
    expect(renderMath("x".repeat(10_001))).toBeNull();
    expect(renderMath("\\href{https://example.com}{link}")).not.toContain(
      "href=",
    );
    expect(
      renderMath("\\includegraphics{https://example.com/image.png}"),
    ).not.toContain("<img");
    expect(renderMath("\\def\\a{\\a}\\a")).toBeNull();
  });

  it("renders Pele SVG with unique IDs and no active links or images", async () => {
    const source =
      'flowchart LR\n accTitle: Test diagram\n A[開始] --> B[完成]\n click A "https://example.com"';
    const first = await renderDiagram(source, "diagram-one");
    const second = await renderDiagram(source, "diagram-two");
    expect(first.svg).toContain("<svg");
    expect(first.svg).toContain("開始");
    expect(first.svg).toContain("diagram-one");
    expect(second.svg).toContain("diagram-two");
    expect(first.svg).not.toMatch(/<a\b|<image\b|<script\b/);
    await expect(renderDiagram("not a diagram", "invalid")).rejects.toThrow();
    await expect(
      renderDiagram("x".repeat(50_001), "oversized"),
    ).rejects.toThrow();
  });
});
