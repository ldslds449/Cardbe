import { mount, tick, unmount } from "svelte";
import Markdown from "../../src/routes/components/markdown.svelte";
import RichTextSurface from "../../src/lib/components/editor/rich-text-surface.svelte";
import PublicShare from "../../src/routes/share/[id]/+page.svelte";

declare global {
  var __cardbeXss: number;
}

const attack = "globalThis.__cardbeXss+=1";
const html = [
  `<script>${attack}</script>`,
  `<img src="data:image/png,invalid" onerror="${attack}">`,
  `<svg onload="${attack}"><a href="javascript:${attack}">svg link</a></svg>`,
  `<iframe srcdoc="<script>parent.__cardbeXss+=1</script>"></iframe>`,
  `<object data="data:text/html,<script>${attack}</script>"></object>`,
  `<a href="javascript:${attack}" onclick="${attack}">html link</a>`,
].join("\n\n");
const unsafeLinks = [
  `[js](javascript:${attack})`,
  `[case](JaVaScRiPt:${attack})`,
  `[entity](javascript&#58;${attack})`,
  `[tab](java&#x09;script:${attack})`,
  `[data](data:text/html;base64,${btoa(`<script>${attack}</script>`)})`,
  `[vb](vbscript:msgbox)`,
];
const markdown = `**safe content**\n\n${html}\n\n${unsafeLinks.join("\n\n")}\n\n[safe link](https://example.com/safe)`;

function check(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message);
  }
}

async function until(predicate: () => unknown) {
  for (let i = 0; i < 200; i++) {
    await tick();
    if (predicate()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error("Timed out waiting for rendered content");
}

async function assertSafe(root: ParentNode) {
  check(
    !root.querySelector("script, iframe, object, embed, [srcdoc]"),
    "Executable HTML survived",
  );
  for (const element of root.querySelectorAll("*")) {
    for (const attribute of element.attributes) {
      check(
        !/^on/i.test(attribute.name),
        `Event attribute survived: ${attribute.name}`,
      );
    }
  }
  for (const anchor of root.querySelectorAll("a")) {
    const href = anchor.getAttribute("href");
    check(
      !href ||
        /^(https?:)$/.test(new URL(anchor.href).protocol) ||
        href.startsWith("#card-"),
      `Unsafe link survived: ${href}`,
    );
    if (!href) {
      anchor.click();
    }
  }
  for (const element of root.querySelectorAll("img, svg, a")) {
    for (const event of ["error", "load", "mouseover"]) {
      element.dispatchEvent(new Event(event));
    }
  }
  await new Promise((resolve) => setTimeout(resolve, 50));
  check(globalThis.__cardbeXss === 0, "An XSS payload executed");
}

export async function run() {
  const results: { name: string; error?: string }[] = [];
  globalThis.__cardbeXss = 0;
  async function test(
    name: string,
    body: (host: HTMLDivElement) => Promise<void>,
  ) {
    const host = document.createElement("div");
    document.body.append(host);
    try {
      await body(host);
      results.push({ name });
    } catch (error) {
      results.push({
        name,
        error: error instanceof Error ? error.stack : String(error),
      });
    } finally {
      host.remove();
    }
  }

  await test("Browser positive control detects script URLs and event handlers", async (host) => {
    const image = document.createElement("img");
    image.setAttribute("onerror", attack);
    host.append(image);
    image.dispatchEvent(new Event("error"));
    check(
      globalThis.__cardbeXss === 1,
      "Inline event detection is not working",
    );
    const link = document.createElement("a");
    link.href = `javascript:void(${attack})`;
    host.append(link);
    link.click();
    await until(() => globalThis.__cardbeXss === 2);
    globalThis.__cardbeXss = 0;
  });

  await test("Markdown preview strips malicious HTML and URL schemes", async (host) => {
    const component = mount(Markdown, {
      target: host,
      props: { md: markdown },
    });
    try {
      await until(() => host.querySelector("strong"));
      check(
        host.querySelector("strong")?.textContent === "safe content",
        "Safe formatting was lost",
      );
      check(
        host.querySelector('a[href="https://example.com/safe"]'),
        "Safe link was lost",
      );
      await assertSafe(host);
    } finally {
      await unmount(component);
    }
  });

  await test("Markdown code keeps attack examples as inert text", async (host) => {
    const component = mount(Markdown, {
      target: host,
      props: { md: `\`\`\`html\n${html}\n\`\`\`` },
    });
    try {
      await until(() => host.querySelector("pre code"));
      check(
        host
          .querySelector("code")
          ?.textContent?.includes(`<script>${attack}</script>`),
        "Code example was lost",
      );
      await assertSafe(host);
    } finally {
      await unmount(component);
    }
  });

  for (const [name, type, content] of [
    [
      "HTML clipboard",
      "text/html",
      `<p>paste marker <strong>safe paste</strong></p>${html}`,
    ],
    ["plain text clipboard", "text/plain", `paste marker ${html}`],
    ["Markdown clipboard", "text/plain", `paste marker\n\n${markdown}`],
  ]) {
    await test(`${name} cannot execute tags, events or URLs`, async (host) => {
      let saved = "";
      const component = mount(RichTextSurface, {
        target: host,
        props: {
          value: "",
          onvaluechange: (value) => {
            saved = value;
          },
        },
      });
      try {
        await until(() => host.querySelector('[contenteditable="true"]'));
        const editor = host.querySelector<HTMLElement>(
          '[contenteditable="true"]',
        )!;
        editor.focus();
        const clipboardData = new DataTransfer();
        clipboardData.setData(type, content);
        editor.dispatchEvent(
          new ClipboardEvent("paste", {
            clipboardData,
            bubbles: true,
            cancelable: true,
          }),
        );
        await until(() => saved.includes("paste marker"));
        if (type === "text/html") {
          check(
            editor.querySelector("strong")?.textContent === "safe paste",
            "Safe pasted formatting was lost",
          );
        }
        await assertSafe(editor);
        const preview = document.createElement("div");
        host.append(preview);
        const rendered = mount(Markdown, {
          target: preview,
          props: { md: saved },
        });
        try {
          await until(() => preview.querySelector(".markdown-content"));
          check(
            preview.textContent?.includes("paste marker"),
            "Pasted content was lost on preview",
          );
          await assertSafe(preview);
        } finally {
          await unmount(rendered);
        }
      } finally {
        await unmount(component);
      }
    });
  }

  await test("Public share snapshot and task detail reject XSS", async (host) => {
    const originalFetch = window.fetch;
    let requests = 0;
    window.fetch = async (input) => {
      check(
        String(input).startsWith("/api/shares/"),
        `Unexpected request: ${input}`,
      );
      requests++;
      return Response.json({
        id: "xss-fixture",
        updated_at: "2026-01-01",
        expires_at: null,
        snapshot: {
          title: `<img onerror="${attack}">shared board`,
          columns: [
            {
              id: 1,
              name: `<svg onload="${attack}">column`,
              color: "",
              tasks: [
                {
                  id: 1,
                  title: `<img onerror="${attack}">task marker`,
                  description: markdown,
                  color: "",
                  labels: [`<svg onload="${attack}">label`],
                  items: [
                    {
                      id: "item-1",
                      text: `<img onerror="${attack}">checklist marker`,
                      completed: false,
                    },
                  ],
                },
              ],
            },
          ],
        },
      });
    };
    const component = mount(PublicShare, { target: host });
    try {
      await until(() => host.querySelector("[data-task-card-surface]"));
      check(requests === 1, "Share fixture was not fetched");
      check(
        host.textContent?.includes("task marker"),
        "Shared task was not rendered",
      );
      await assertSafe(host);
      host.querySelector<HTMLElement>("[data-task-card-surface]")!.click();
      await until(() =>
        document.querySelector('[role="dialog"] .markdown-content strong'),
      );
      const dialog = document.querySelector('[role="dialog"]')!;
      check(
        dialog.textContent?.includes("safe content"),
        "Shared description was not rendered",
      );
      check(
        dialog.textContent?.includes("checklist marker"),
        "Shared checklist was not rendered",
      );
      await assertSafe(dialog);
    } finally {
      await unmount(component);
      window.fetch = originalFetch;
    }
  });
  return results;
}
