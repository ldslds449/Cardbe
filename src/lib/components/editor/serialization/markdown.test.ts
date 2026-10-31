import assert from "node:assert/strict";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { createServer as createNetServer } from "node:net";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vite-plus/test";
import { Editor } from "@tiptap/core";
import type { Transaction } from "@tiptap/pm/state";

import { createEditorExtensions } from "../extensions";
import { codeLanguages } from "../extensions/code-highlight";
import {
  markdownInsertionContent,
  serializeDocument,
  serializeMarkdown,
} from "./markdown";

const markdown = [
  "# Heading",
  "",
  "A **bold** and _italic_ ~~struck~~ [link](https://example.com) with `code`.",
  "",
  "- bullet",
  "- [x] done",
  "",
  "1. ordered",
  "",
  "> quote",
  "",
  "```rust",
  "fn main() {",
  '    println!("hello");',
  "}",
  "```",
  "",
  "---",
].join("\n");

const editors: Editor[] = [];

afterEach(() => {
  for (const editor of editors.splice(0)) {
    editor.destroy();
  }
});

describe("Markdown editor serialization", () => {
  it("keeps user text resembling formula placeholders", () => {
    const source = "Literal \uE0010\uE002 and $x$";
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(serializeMarkdown(editor)).toBe(source);
  });
  it.each(["inlineMath", "blockMath"])(
    "keeps dollar characters entered in a %s formula",
    (type) => {
      const editor = new Editor({
        element: null,
        extensions: createEditorExtensions(),
        content: { type: "doc", content: [{ type: "paragraph" }] },
      });
      editors.push(editor);
      const formula = { type, attrs: { latex: "\\$ + $$" } };
      editor.commands.setContent({
        type: "doc",
        content:
          type === "inlineMath"
            ? [
                {
                  type: "paragraph",
                  content: [{ type: "text", text: "Before " }, formula],
                },
              ]
            : [formula],
      });
      const reloaded = new Editor({
        element: null,
        extensions: createEditorExtensions(),
        content: serializeMarkdown(editor),
        contentType: "markdown",
      });
      editors.push(reloaded);
      const content = reloaded.getJSON().content?.[0];
      expect(
        type === "inlineMath" ? content?.content?.[1] : content,
      ).toMatchObject(formula);
      expect(serializeMarkdown(reloaded)).toBe(serializeMarkdown(editor));
    },
  );
  it.each([
    "Inline $$x^2$$ and $y$",
    "Inline $$a$b$$",
    "Inline $a\nb$",
    "**Bold $x^2$** and [formula $y$](https://example.com)",
    "# Heading $x^2$",
    "- Formula $x^2$\n- Next",
  ])("keeps formula syntax stable: %s", (source) => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(serializeMarkdown(editor)).toBe(source);
    const reloaded = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: serializeMarkdown(editor),
      contentType: "markdown",
    });
    editors.push(reloaded);
    expect(reloaded.getJSON()).toEqual(editor.getJSON());
  });
  it("preserves formulas and Mermaid after editing surrounding text", () => {
    const source =
      "公式 $x^2$ 😀\n\n$$\n\\begin{aligned}\na &= b \\\\\n c &= d\n\\end{aligned}\n$$\n\n```mermaid\nflowchart LR\n A[開始] --> B[完成]\n```";
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(editor.getJSON().content?.map((node) => node.type)).toEqual([
      "paragraph",
      "blockMath",
      "codeBlock",
    ]);
    expect(serializeMarkdown(editor)).toBe(source);
    editor.commands.insertContentAt(1, { type: "text", text: "Edited " });
    expect(serializeMarkdown(editor)).toBe(`Edited ${source}`);
    const reloaded = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: serializeMarkdown(editor),
      contentType: "markdown",
    });
    editors.push(reloaded);
    expect(reloaded.getJSON()).toEqual(editor.getJSON());
  });

  it("keeps literal dollars escaped outside formulas and code", () => {
    const source = "\\$literal\\$ and `$code$`\n\n```text\n$code$\n```";
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(serializeMarkdown(editor)).toBe(source);
    expect(
      editor
        .getJSON()
        .content?.[0]?.content?.every((node) => node.type === "text"),
    ).toBe(true);
  });

  it("accepts link destinations without requiring an absolute URL", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Selected text",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 14 });
    for (const href of [
      "example.com",
      "../notes",
      "#section",
      "mailto:hello@example.com",
    ]) {
      expect(editor.chain().setLink({ href }).run()).toBe(true);
      expect(editor.getAttributes("link").href).toBe(href);
    }
  });

  it("rejects unsafe URLs without changing the selected text", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Selected text",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 14 });
    expect(editor.chain().setLink({ href: "javascript:alert(1)" }).run()).toBe(
      false,
    );
    expect(serializeMarkdown(editor)).toBe("Selected text");
  });

  it("adds a link to the saved selection after focus moves to the URL dialog", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Selected text and other text.",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection({ from: 1, to: 14 });
    const selection = {
      from: editor.state.selection.from,
      to: editor.state.selection.to,
    };
    editor.commands.setTextSelection(20);
    editor
      .chain()
      .setTextSelection(selection)
      .setLink({ href: "https://example.com" })
      .run();
    expect(serializeMarkdown(editor)).toBe(
      "[Selected text](https://example.com) and other text.",
    );
    editor.commands.unsetLink();
    expect(serializeMarkdown(editor)).toBe("Selected text and other text.");
  });

  it("serializes a changed code language without changing the code", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "```javascript\nconst value = 1;\n```",
      contentType: "markdown",
    });
    editors.push(editor);
    expect(codeLanguages).toContain("typescript");
    editor.commands.setTextSelection(1);
    editor.commands.updateAttributes("codeBlock", { language: "typescript" });
    expect(serializeMarkdown(editor)).toBe(
      "```typescript\nconst value = 1;\n```",
    );
  });
  it("imports and exports the supported Markdown blocks without losing language metadata", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: markdown,
      contentType: "markdown",
    });
    editors.push(editor);

    const document = editor.getJSON();
    expect(document.content?.map((node) => node.type)).toEqual([
      "heading",
      "paragraph",
      "bulletList",
      "taskList",
      "orderedList",
      "blockquote",
      "codeBlock",
      "horizontalRule",
    ]);
    expect(document.content?.[6]?.attrs?.language).toBe("rust");

    const output = serializeMarkdown(editor);
    expect(output).toContain("# Heading");
    expect(output).toContain("**bold**");
    expect(output).toContain("- [x] done");
    expect(output).toContain("```rust");
    expect(output).toContain('println!("hello");');
    expect(output).toContain("---");
  });

  it("round-trips GFM tables with alignment and inline formatting", () => {
    const source = [
      "| Name | Count | Note |",
      "| :--- | ---: | :---: |",
      "| **Alpha** | 1 | [link](https://example.com) |",
      "| Beta | 22 | `code` |",
    ].join("\n");
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(editor.state.doc.firstChild?.type.name).toBe("table");
    const output = serializeMarkdown(editor);
    const reparsed = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: output,
      contentType: "markdown",
    });
    editors.push(reparsed);
    expect(reparsed.getJSON()).toEqual(editor.getJSON());
    expect(output).toContain("**Alpha**");
    expect(output).toContain("[link](https://example.com)");
    expect(output).toMatch(/\| :-+ \| -+: \| :-+: \|/);
  });

  it("inserts and edits tables as GFM Markdown", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Intro",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection(6);
    expect(
      editor
        .chain()
        .insertTable({ rows: 2, cols: 2, withHeaderRow: true })
        .run(),
    ).toBe(true);
    editor.commands.insertContent({ type: "text", text: "Head" });
    expect(editor.isActive("table")).toBe(true);
    expect(editor.chain().addColumnAfter().addRowAfter().run()).toBe(true);
    const lines = serializeMarkdown(editor)
      .split("\n")
      .filter((line) => line.startsWith("|"));
    expect(lines).toHaveLength(4);
    expect(lines[0]).toMatch(/^\| Head +\| +\| +\|$/);
    expect(lines[1]).toMatch(/^\| -+ \| -+ \| -+ \|$/);
    expect(editor.chain().deleteTable().run()).toBe(true);
    expect(editor.isActive("table")).toBe(false);
  });

  it("aligns the whole column as GFM stores it", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "| A | B |\n| --- | :---: |\n| 1 | 2 |\n| 3 | 4 |",
      contentType: "markdown",
    });
    editors.push(editor);
    // Cursor in the body cell "3" of the first column.
    let position = 0;
    editor.state.doc.descendants((node, pos) => {
      if (node.isText && node.text === "3") {
        position = pos;
      }
    });
    editor.commands.setTextSelection(position);
    expect(editor.chain().setColumnAlign("right").run()).toBe(true);
    let lines = serializeMarkdown(editor).trim().split("\n");
    expect(lines[1]).toMatch(/^\| -+: \| :-+: \|$/);
    expect(editor.getAttributes("tableCell").align).toBe("right");

    expect(editor.chain().setColumnAlign(null).run()).toBe(true);
    lines = serializeMarkdown(editor).trim().split("\n");
    expect(lines[1]).toMatch(/^\| -+ \| :-+: \|$/);

    editor.commands.setTextSelection(editor.state.doc.content.size);
    editor.commands.insertContentAt(editor.state.doc.content.size, {
      type: "paragraph",
      content: [{ type: "text", text: "After" }],
    });
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    expect(editor.isActive("table")).toBe(false);
    expect(editor.can().setColumnAlign("left")).toBe(false);
  });

  it.each([
    ["block image", "![alt text](https://example.com/a.png)"],
    [
      "inline image with title",
      'See ![icon](https://example.com/i.png "Icon") here',
    ],
    ["footnotes", "Text with note[^1].\n\n[^1]: The note."],
    [
      "multi-line footnote",
      "Ref[^long]\n\n[^long]: First line\n  continued\n\n  Second paragraph",
    ],
    ["inline HTML", "before <b>bold</b> after<br/>"],
    ["HTML comment", "text <!-- hidden --> text"],
    ["HTML block", "<div>\nblock\n</div>"],
    ["escaped ordered list marker", "1\\. not a list"],
    ["escaped heading", "\\# not a heading"],
    ["escaped bullets", "\\- not a list\n\n\\+ not a list"],
    ["escaped marker after a line break", "first  \n2\\) not a list"],
    ["escaped setext underline", "text  \n\\---"],
  ])("preserves %s through an edit", (_, source) => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(serializeMarkdown(editor).trim()).toBe(source);
    // Editing elsewhere re-serializes the whole document.
    editor.commands.insertContentAt(editor.state.doc.content.size, {
      type: "paragraph",
      content: [{ type: "text", text: "Edited" }],
    });
    expect(serializeMarkdown(editor).trim()).toBe(`${source}\n\nEdited`);
  });

  it("keeps ordinary text containing block characters unescaped", () => {
    const source = "Price 1. 2 - 3 # tag + 4";
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.insertContentAt(editor.state.doc.content.size, {
      type: "paragraph",
    });
    expect(serializeMarkdown(editor).trim()).toBe(source);
  });

  it("does not treat autolinks as raw HTML", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "<https://example.com>",
      contentType: "markdown",
    });
    editors.push(editor);
    expect(editor.isActive("rawHtmlInline")).toBe(false);
    let hasLink = false;
    editor.state.doc.descendants((node) => {
      hasLink ||= node.marks.some((mark) => mark.type.name === "link");
    });
    expect(hasLink).toBe(true);
  });

  it("keeps table cells to one line", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "| A |\n| --- |\n| B |",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.setTextSelection(4);
    expect(editor.isActive("tableHeader")).toBe(true);
    editor.commands.splitBlock();
    expect(serializeMarkdown(editor)).not.toContain("<br>");
  });

  it("keeps an empty document editable", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: { type: "doc", content: [{ type: "paragraph" }] },
    });
    editors.push(editor);

    expect(editor.isEmpty).toBe(true);
    expect(serializeMarkdown(editor)).toBe("");
  });

  it("matches whole-document serialization while reusing unchanged blocks", () => {
    const source = [
      markdown,
      "&nbsp;",
      "&nbsp;",
      "3. third\n4. fourth",
      "| A | B |\n| :- | -: |\n| `x` | **y** |",
      "Price $5 and $x^2$ with \\[[brackets]] 1\\. not a list",
      "$$\nE = mc^2\n$$",
      "Text[^1] with <kbd>raw</kbd>\n\n[^1]: Footnote",
      "![image](https://example.com/a.png)",
      "[[Other card|task_12]] reference",
      "```mermaid\nflowchart TD\n  A --> B\n```",
    ].join("\n\n");
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    const check = () =>
      expect(serializeMarkdown(editor)).toBe(serializeDocument(editor));
    check();
    const blockStart = (index: number) => {
      let start = 0;
      editor.state.doc.forEach((_node, offset, position) => {
        if (position === index) {
          start = offset;
        }
      });
      return start;
    };
    const change = (apply: (tr: Transaction) => void) =>
      editor.commands.command(({ tr }) => {
        apply(tr);
        return true;
      });
    const paragraph = () => editor.schema.nodes.paragraph.create();
    const edits = [
      () => change((tr) => tr.insertText("typed ", blockStart(1) + 2)),
      () => change((tr) => tr.insert(blockStart(0), paragraph())),
      () =>
        change((tr) => tr.insert(blockStart(3), [paragraph(), paragraph()])),
      () => change((tr) => tr.split(blockStart(2) + 3)),
      () =>
        change((tr) => {
          const start = blockStart(4);
          tr.delete(start, start + tr.doc.child(4).nodeSize);
        }),
      () => change((tr) => tr.insertText("$y ", blockStart(6) + 1)),
    ];
    for (const edit of edits) {
      edit();
      check();
    }
  });

  it("preserves card references through Markdown serialization", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "See [[My card|task_123]] and [ordinary](https://example.com).",
      contentType: "markdown",
    });
    editors.push(editor);
    expect(editor.getJSON().content?.[0]?.content?.[1]).toMatchObject({
      type: "cardReference",
      attrs: { title: "My card", taskId: "task_123" },
    });
    expect(serializeMarkdown(editor)).toBe(
      "See [[My card|task_123]] and [ordinary](https://example.com).",
    );
  });
  it.each(["[待辦] 修正問題", "Closing ] bracket", "Repeated ]] brackets"])(
    "preserves brackets in a reference title through rich editing: %s",
    (title) => {
      const source = `Before [[${title}|task_123]] after`;
      const editor = new Editor({
        element: null,
        extensions: createEditorExtensions(),
        content: source,
        contentType: "markdown",
      });
      editors.push(editor);
      expect(editor.getJSON().content?.[0]?.content?.[1]).toMatchObject({
        type: "cardReference",
        attrs: { title, taskId: "task_123" },
      });
      expect(serializeMarkdown(editor)).toBe(source);
    },
  );

  it("inserts a card reference inline without splitting its paragraph", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: "Before [[ after",
      contentType: "markdown",
    });
    editors.push(editor);
    editor.commands.insertContentAt(
      { from: 8, to: 10 },
      markdownInsertionContent(editor, "[[Card|task_123]]"),
    );
    expect(editor.getJSON().content).toHaveLength(1);
    expect(serializeMarkdown(editor)).toBe("Before [[Card|task_123]] after");
  });
  it("keeps card reference syntax literal inside code", () => {
    const source = "`[[Card|task_123]]`\n\n```text\n[[Card|task_123]]\n```";
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: source,
      contentType: "markdown",
    });
    editors.push(editor);
    expect(editor.getJSON().content?.[0]?.content?.[0]?.type).toBe("text");
    expect(serializeMarkdown(editor)).toBe(source);
  });
  it("keeps multiple Markdown paragraphs as blocks when inserting", () => {
    const editor = new Editor({
      element: null,
      extensions: createEditorExtensions(),
      content: { type: "doc", content: [{ type: "paragraph" }] },
    });
    editors.push(editor);
    editor.commands.insertContentAt(
      1,
      markdownInsertionContent(editor, "First\n\nSecond"),
    );
    expect(serializeMarkdown(editor)).toBe("First\n\nSecond");
  });
});

it("blocks XSS in Markdown previews, clipboard content and public shares", async ({
  skip,
}) => {
  // Exercise the real browser DOM without adding a browser automation dependency.
  const root = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "../../../../..",
  );

  const browserCandidates = process.env.CARDBE_TEST_BROWSER
    ? [process.env.CARDBE_TEST_BROWSER]
    : [
        "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
        "C:/Program Files/Google/Chrome/Application/chrome.exe",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
      ];
  const browser = browserCandidates.find((candidate) => existsSync(candidate));
  if (!browser) {
    skip("No Chromium browser found; browser XSS checks were not run.");
    return;
  }

  const probe = createNetServer();
  await new Promise<void>((resolve) => probe.listen(0, "127.0.0.1", resolve));
  const address = probe.address();
  assert(address && typeof address !== "string");
  const port = address.port;
  await new Promise<void>((resolve, reject) =>
    probe.close((error) => (error ? reject(error) : resolve())),
  );
  const previousPort = process.env.CARDBE_DEV_PORT;
  process.env.CARDBE_DEV_PORT = String(port);
  const { createServer } = await import("vite-plus");
  type Report = { name: string; error?: string }[];
  let resolveResult!: (report: Report) => void;
  let rejectResult!: (error: unknown) => void;
  const result = new Promise<Report>((resolve, reject) => {
    resolveResult = resolve;
    rejectResult = reject;
  });
  const server = await createServer({
    root,
    server: { open: false, fs: { allow: [root] } },
    plugins: [
      {
        name: "xss-browser-tests",
        configureServer(vite) {
          vite.middlewares.use(async (request, response, next) => {
            if (request.url === "/__xss.html") {
              response.setHeader("Content-Type", "text/html");
              response.end(
                await vite.transformIndexHtml(
                  request.url,
                  readFileSync(
                    path.join(root, "scripts/test-fixtures/xss.html"),
                    "utf8",
                  ),
                ),
              );
            } else if (
              request.url === "/__xss_result" &&
              request.method === "POST"
            ) {
              let body = "";
              request.on("data", (chunk) => {
                body += chunk;
              });
              request.on("end", () => {
                response.end("ok");
                try {
                  resolveResult(JSON.parse(body));
                } catch (error) {
                  rejectResult(error);
                }
              });
            } else {
              next();
            }
          });
        },
      },
    ],
  });
  const profile = mkdtempSync(path.join(tmpdir(), "cardbe-xss-"));
  let child: ChildProcess | undefined;
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    await server.listen();
    timeout = setTimeout(
      () => rejectResult(new Error("XSS browser tests timed out")),
      90_000,
    );
    child = spawn(
      browser,
      [
        "--headless",
        "--disable-extensions",
        "--disable-background-networking",
        "--disable-sync",
        "--no-first-run",
        "--no-default-browser-check",
        `--user-data-dir=${profile}`,
        `http://127.0.0.1:${port}/__xss.html`,
      ],
      { stdio: ["ignore", "ignore", "pipe"], windowsHide: true },
    );
    child.on("error", rejectResult);
    child.stderr!.on("data", (chunk) => process.stderr.write(chunk));
    child.on("exit", (code) =>
      rejectResult(
        new Error(`Browser exited before reporting results (${code})`),
      ),
    );
    const report = await result;
    for (const entry of report) {
      console.log(
        `${entry.error ? "FAIL" : "PASS"} ${entry.name}${entry.error ? `: ${entry.error}` : ""}`,
      );
    }
    assert(report.length === 7, "The browser must run every XSS scenario");
    assert(!report.some((entry) => entry.error), "XSS regression tests failed");
  } finally {
    clearTimeout(timeout);
    if (previousPort === undefined) {
      delete process.env.CARDBE_DEV_PORT;
    } else {
      process.env.CARDBE_DEV_PORT = previousPort;
    }
    if (child?.pid) {
      if (process.platform === "win32") {
        spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], {
          windowsHide: true,
          stdio: "ignore",
        });
      } else if (child.exitCode === null && child.signalCode === null) {
        const exited = new Promise<void>((resolve) =>
          child!.once("exit", () => resolve()),
        );
        child.kill();
        await exited;
      }
    }
    await server.close();
    // The only directory removed is the fresh temporary browser profile above.
    rmSync(profile, {
      recursive: true,
      force: true,
      maxRetries: 5,
      retryDelay: 200,
    });
  }
}, 160_000);
