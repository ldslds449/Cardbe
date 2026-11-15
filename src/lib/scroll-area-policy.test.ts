import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vite-plus/test";

const SRC = join(process.cwd(), "src");
const SKIP = new Set(["paraglide", "node_modules"]);
const FORBIDDEN =
  /overflow-(?:auto|scroll|x-auto|y-auto|x-scroll|y-scroll)\b|scrollbar-width|::-webkit-scrollbar|\[&::-webkit-scrollbar\]/;

function* sources(dir: string): Generator<string> {
  for (const name of readdirSync(dir)) {
    if (SKIP.has(name)) {
      continue;
    }
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      yield* sources(path);
    } else if (/\.(svelte|css)$/.test(name)) {
      yield path;
    }
  }
}

describe("scroll area policy", () => {
  it("uses ScrollArea instead of native scrollbars", () => {
    const offenders: string[] = [];
    for (const file of sources(SRC)) {
      readFileSync(file, "utf8")
        .split("\n")
        .forEach((line, index) => {
          if (FORBIDDEN.test(line)) {
            offenders.push(`${relative(SRC, file)}:${index + 1}`);
          }
        });
    }
    expect(offenders).toEqual([]);
  });
});
