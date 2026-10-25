import type { HastNode } from "svelte-exmarkdown";

function text(node: HastNode): string {
  return node.value ?? node.children?.map(text).join("") ?? "";
}

// Run before syntax highlighting so code text and math delimiters stay intact.
export function markdownSpecial(markdown = "") {
  return (tree: HastNode) => {
    function visit(node: HastNode) {
      const code =
        node.type === "element" && node.tagName === "pre"
          ? node.children?.[0]
          : node;
      if (code?.type === "element" && code.tagName === "code") {
        const classes = code.properties?.className;
        if (Array.isArray(classes)) {
          const mermaid = classes.includes("language-mermaid");
          const math =
            classes.includes("math-inline") || classes.includes("math-display");
          if (mermaid || math) {
            const offset = code.position?.start.offset;
            Object.assign(node, {
              type: "element",
              tagName: mermaid ? "mermaid-diagram" : "math-formula",
              properties: {
                source:
                  node.type === "element" && node.tagName === "pre" && mermaid
                    ? text(code).replace(/\n$/, "")
                    : text(code),
                display:
                  classes.includes("math-display") ||
                  (math &&
                    offset !== undefined &&
                    markdown.slice(offset).startsWith("$$")),
              },
              children: [],
            });
            return;
          }
        }
      }
      node.children?.forEach(visit);
      // Display math must sit outside paragraphs, including single-line $$...$$.
      if (node.children) {
        node.children = node.children.flatMap((child) => {
          if (
            child.type !== "element" ||
            child.tagName !== "p" ||
            !child.children.some(
              (content) =>
                content.type === "element" &&
                content.tagName === "math-formula" &&
                content.properties?.display,
            )
          ) {
            return [child];
          }
          const blocks: HastNode[] = [];
          let inline: typeof child.children = [];
          const flush = () => {
            if (
              inline.some(
                (content) => content.type !== "text" || content.value.trim(),
              )
            ) {
              blocks.push({ ...child, children: inline });
            }
            inline = [];
          };
          for (const content of child.children) {
            if (
              content.type === "element" &&
              content.tagName === "math-formula" &&
              content.properties?.display
            ) {
              flush();
              blocks.push(content);
            } else {
              inline.push(content);
            }
          }
          flush();
          return blocks;
        });
      }
    }
    visit(tree);
  };
}
