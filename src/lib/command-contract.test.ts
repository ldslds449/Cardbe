import { describe, expect, it } from "vite-plus/test";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = fileURLToPath(new URL("../../", import.meta.url));

function files(directory: string, extensions: string[]): string[] {
  return readdirSync(join(repo, directory), { recursive: true })
    .map((file) => join(directory, file.toString()))
    .filter((file) => extensions.some((extension) => file.endsWith(extension)));
}

function read(file: string): string {
  return readFileSync(join(repo, file), "utf8").replaceAll("\r\n", "\n");
}

const PAIRS: Record<string, string> = { "(": ")", "[": "]", "{": "}" };

/** Index just past a string, template or comment starting at `index`. */
function skip_literal(text: string, index: number): number | null {
  const char = text[index];
  if (char === '"' || char === "'") {
    let cursor = index + 1;
    while (cursor < text.length && text[cursor] !== char) {
      cursor += text[cursor] === "\\" ? 2 : 1;
    }
    return cursor + 1;
  }
  if (char === "`") {
    let cursor = index + 1;
    while (cursor < text.length && text[cursor] !== "`") {
      if (text[cursor] === "\\") {
        cursor += 2;
      } else if (text.startsWith("${", cursor)) {
        cursor = find_close(text, cursor + 1) + 1;
      } else {
        cursor++;
      }
    }
    return cursor + 1;
  }
  if (text.startsWith("//", index)) {
    const end = text.indexOf("\n", index);
    return end === -1 ? text.length : end;
  }
  if (text.startsWith("/*", index)) {
    return text.indexOf("*/", index + 2) + 2;
  }
  return null;
}

/** Index of the bracket closing the one at `open`. */
function find_close(text: string, open: number): number {
  const expected = [PAIRS[text[open]]];
  let index = open + 1;
  while (index < text.length) {
    const skipped = skip_literal(text, index);
    if (skipped !== null) {
      index = skipped;
      continue;
    }
    const char = text[index];
    if (char in PAIRS) {
      expected.push(PAIRS[char]);
    } else if (char === expected.at(-1)) {
      expected.pop();
      if (expected.length === 0) {
        return index;
      }
    }
    index++;
  }
  throw new Error(`Unbalanced ${text[open]} at ${open}`);
}

/** Split TypeScript on commas outside brackets, strings and comments. */
function split_arguments(text: string): string[] {
  const parts: string[] = [];
  let start = 0;
  let index = 0;
  while (index < text.length) {
    const skipped = skip_literal(text, index);
    if (skipped !== null) {
      index = skipped;
    } else if (text[index] in PAIRS) {
      index = find_close(text, index) + 1;
    } else if (text[index] === ",") {
      parts.push(text.slice(start, index));
      start = ++index;
    } else {
      index++;
    }
  }
  parts.push(text.slice(start));
  return parts.map((part) => part.trim()).filter(Boolean);
}

/** Index just past a TypeScript type-argument list starting at `open`. */
function skip_type_arguments(text: string, open: number): number {
  let depth = 0;
  let index = open;
  while (index < text.length) {
    const char = text[index];
    if (char in PAIRS) {
      index = find_close(text, index) + 1;
      continue;
    }
    if (char === "<") {
      depth++;
    } else if (char === ">" && text[index - 1] !== "=") {
      depth--;
      if (depth === 0) {
        return index + 1;
      }
    }
    index++;
  }
  throw new Error(`Unbalanced type arguments at ${open}`);
}

/** Split a Rust parameter list on commas outside generic and other brackets. */
function split_rust_parameters(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if ("<([{".includes(char)) {
      depth++;
    } else if (">)]}".includes(char) && text[index - 1] !== "-") {
      depth--;
    } else if (char === "," && depth === 0) {
      parts.push(text.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(text.slice(start));
  return parts.map((part) => part.trim()).filter(Boolean);
}

/** Property name of an object-literal entry, or null for a spread. */
function argument_key(entry: string): string | null {
  const code = entry.replace(/^(?:\s*(?:\/\/[^\n]*|\/\*[\s\S]*?\*\/))+\s*/, "");
  return code.startsWith("...")
    ? null
    : (/^["']?(\w+)/.exec(code)?.[1] ?? code);
}

/** Command names a literal or a `condition ? "a" : "b"` expression selects. */
function command_names(expression: string): string[] {
  const literal = /^"(\w+)"$/.exec(expression);
  const choice = /\?\s*"(\w+)"\s*:\s*"(\w+)"$/.exec(expression);
  return literal ? [literal[1]] : choice ? [choice[1], choice[2]] : [];
}

const camel_case = (name: string) =>
  name.replace(/_([a-z0-9])/g, (_, char: string) => char.toUpperCase());

/** Arguments the frontend sends, mapped to whether each is required. */
function rust_commands(): Map<string, Map<string, boolean>> {
  const commands = new Map<string, Map<string, boolean>>();
  for (const file of files("src-tauri/src", [".rs"])) {
    for (const item of read(file).split("#[tauri::command]").slice(1)) {
      const name = /fn\s+(\w+)/.exec(item);
      if (!name) {
        throw new Error(`No function after a command attribute in ${file}`);
      }
      const open = item.indexOf("(", name.index);
      let depth = 0;
      let close = open;
      for (; close < item.length; close++) {
        depth += item[close] === "(" ? 1 : item[close] === ")" ? -1 : 0;
        if (depth === 0) {
          break;
        }
      }
      const parameters = new Map<string, boolean>();
      for (const parameter of split_rust_parameters(
        item.slice(open + 1, close),
      )) {
        const [binding, type] = parameter.split(/:(.*)/s).map((s) => s.trim());
        if (
          /^(?:tauri::)?(?:State|AppHandle|WebviewWindow|Window)\b/.test(type)
        ) {
          continue;
        }
        parameters.set(
          camel_case(binding.replace(/^mut\s+/, "")),
          !type.startsWith("Option<"),
        );
      }
      commands.set(name[1], parameters);
    }
  }
  return commands;
}

function registered_commands(): Set<string> {
  const handler = read("src-tauri/src/lib.rs")
    .split("generate_handler![")[1]
    .split("])")[0];
  return new Set(
    handler
      .split(",")
      .map((path) => path.trim().split("::").at(-1) ?? "")
      .filter(Boolean),
  );
}

interface FrontendCall {
  location: string;
  names: string[];
  /** Keys of a literal argument object, or null when not a literal. */
  keys: string[] | null;
  spread: boolean;
  /** A Tauri plugin command such as `plugin:window-state|save_window_state`. */
  plugin: boolean;
}

function frontend_calls(): FrontendCall[] {
  const calls: FrontendCall[] = [];
  for (const file of files("src", [".ts", ".svelte"])) {
    if (file.includes("paraglide") || file.endsWith(".test.ts")) {
      continue;
    }
    const text = read(file);
    for (const match of text.matchAll(/\binvoke\s*(?=[<(])/g)) {
      let index = match.index + match[0].length;
      if (text[index] === "<") {
        index = skip_type_arguments(text, index);
      }
      const close = find_close(text, index);
      const [name, args] = split_arguments(text.slice(index + 1, close));
      const line = text.slice(0, match.index).split("\n").length;
      const call: FrontendCall = {
        location: `${file}:${line}`,
        names: command_names(name),
        keys: [],
        spread: false,
        plugin: /^"plugin:[\w-]+\|\w+"$/.test(name),
      };
      if (args?.startsWith("{") && args.endsWith("}")) {
        for (const entry of split_arguments(args.slice(1, -1))) {
          const key = argument_key(entry);
          if (key === null) {
            call.spread = true;
          } else {
            call.keys?.push(key);
          }
        }
      } else if (args !== undefined) {
        call.keys = null;
      }
      calls.push(call);
    }
  }
  return calls;
}

describe("frontend and backend command contract", () => {
  const commands = rust_commands();
  const registered = registered_commands();
  const calls = frontend_calls();

  it("finds the commands and their frontend callers", () => {
    expect(commands.size).toBeGreaterThan(80);
    expect(calls.length).toBeGreaterThan(100);
  });

  it("registers every command and nothing else", () => {
    expect([...registered].filter((name) => !commands.has(name))).toEqual([]);
    expect(
      [...commands.keys()].filter((name) => !registered.has(name)),
    ).toEqual([]);
  });

  it("calls only registered commands with their Rust argument names", () => {
    const failures: string[] = [];
    for (const call of calls) {
      if (call.plugin) {
        continue;
      }
      if (call.names.length === 0) {
        failures.push(
          `${call.location}: command name is not a literal or literal choice`,
        );
      }
      for (const name of call.names) {
        const parameters = commands.get(name);
        if (!parameters || !registered.has(name)) {
          failures.push(`${call.location}: unknown command ${name}`);
          continue;
        }
        for (const key of call.keys ?? []) {
          if (!parameters.has(key)) {
            failures.push(`${call.location}: ${name} has no argument ${key}`);
          }
        }
        if (call.keys && !call.spread && call.names.length === 1) {
          for (const [parameter, required] of parameters) {
            if (required && !call.keys.includes(parameter)) {
              failures.push(
                `${call.location}: ${name} is missing argument ${parameter}`,
              );
            }
          }
        }
      }
    }
    expect(failures).toEqual([]);
  });

  it("calls every registered command from the frontend", () => {
    const called = new Set(calls.flatMap((call) => call.names));
    const uncalled = [...registered].filter((name) => !called.has(name));
    expect(uncalled).toEqual([]);
  });

  it("checks argument objects and type arguments without being misled", () => {
    const text = `invoke<{ run: () => void }>("cmd", {
      a,
      // b: comment,
      "c": \`\${[1, 2].join(",")}\`,
      ...rest,
    })`;
    const open = skip_type_arguments(text, text.indexOf("<"));
    const [name, args] = split_arguments(
      text.slice(open + 1, find_close(text, open)),
    );
    expect(command_names(name)).toEqual(["cmd"]);
    expect(split_arguments(args.slice(1, -1)).map(argument_key)).toEqual([
      "a",
      "c",
      null,
    ]);
    expect(
      command_names(
        'kind === "package"\n ? "remove_package"\n : "remove_item"',
      ),
    ).toEqual(["remove_package", "remove_item"]);
    expect(command_names("name")).toEqual([]);
    expect(split_rust_parameters("a: Vec<(i64, u8)>, b: [String; 5]")).toEqual([
      "a: Vec<(i64, u8)>",
      "b: [String; 5]",
    ]);
  });
});
