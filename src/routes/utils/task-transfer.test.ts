import { describe, expect, it } from "vite-plus/test";

import {
  create_task,
  create_task_item,
  TASK_PRIORITIES,
} from "../type/task.svelte";
import { parse_portable_task, serialize_portable_task } from "./task-transfer";

async function transform(
  bytes: Uint8Array,
  stream: CompressionStream | DecompressionStream,
): Promise<Uint8Array> {
  const output = new Response(stream.readable).arrayBuffer();
  const writer = stream.writable.getWriter();
  await writer.write(bytes as Uint8Array<ArrayBuffer>);
  await writer.close();
  return new Uint8Array(await output);
}
function decode_base64url(value: string): Uint8Array {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(base64.padEnd(Math.ceil(base64.length / 4) * 4, "="));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}
function encode_base64url(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

describe("task transfer", () => {
  it("round trips a task without retaining its id", async () => {
    const item = create_task_item("Ship it");
    const task = create_task(
      "task_42",
      "Release",
      "Final checks",
      "#ff0000",
      new Date(1000),
      new Date(2000),
      ["work"],
      [item],
      { frequency: "weekly", interval: 2 },
    );

    const shared = await serialize_portable_task(task);
    const imported = await parse_portable_task(shared);

    expect(shared).toMatch(/^cardbe-task:v1:[A-Za-z0-9_-]+$/);
    expect(shared).not.toContain("Release");
    expect(imported.id).toBe("");
    expect(imported.title).toBe("Release");
    expect(imported.start_time?.getTime()).toBe(1000);
    expect(imported.due_time?.getTime()).toBe(2000);
    expect(imported.labels).toEqual(["work"]);
    expect(imported.recurrence).toEqual({ frequency: "weekly", interval: 2 });
    expect(imported.items[0].text).toBe("Ship it");
    expect(imported.items[0].id).not.toBe(item.id);
  });

  it("drops the creation time that version 1 codes stored as start time", async () => {
    const shared = await serialize_portable_task(
      create_task("task_1", "Old", "", "", new Date(1000), new Date(2000)),
    );
    const prefix = "cardbe-task:v1:";
    const bytes = await transform(
      decode_base64url(shared.slice(prefix.length)),
      new DecompressionStream("gzip"),
    );
    bytes[0] = 1;
    const legacy =
      prefix +
      encode_base64url(await transform(bytes, new CompressionStream("gzip")));

    const imported = await parse_portable_task(legacy);
    expect(imported.title).toBe("Old");
    expect(imported.start_time).toBeUndefined();
    expect(imported.due_time?.getTime()).toBe(2000);
  });

  it("keeps a chosen midnight", async () => {
    const task = create_task(
      "",
      "Midnight",
      "",
      "",
      new Date(2026, 9, 13),
      new Date(2026, 9, 14),
    );
    const plain = await parse_portable_task(
      await serialize_portable_task(task),
    );
    expect(plain.start_at_midnight).toBeUndefined();
    expect(plain.due_at_midnight).toBeUndefined();

    task.due_at_midnight = true;
    const imported = await parse_portable_task(
      await serialize_portable_task(task),
    );
    expect(imported.start_at_midnight).toBeUndefined();
    expect(imported.due_at_midnight).toBe(true);
    expect(imported.due_time).toEqual(new Date(2026, 9, 14));
  });

  it("keeps the priority alongside chosen midnights", async () => {
    for (const priority of TASK_PRIORITIES) {
      const task = create_task("", "Ranked", "", "", undefined, new Date(0));
      task.priority = priority;
      task.due_at_midnight = priority === "high";
      const imported = await parse_portable_task(
        await serialize_portable_task(task),
      );
      expect(imported.priority).toBe(priority);
      expect(imported.due_at_midnight).toBe(
        priority === "high" ? true : undefined,
      );
    }
  });

  it("ignores priority codes from newer versions", async () => {
    const task = create_task("", "Future");
    task.priority = "urgent";
    const shared = await serialize_portable_task(task);
    const prefix = "cardbe-task:v1:";
    const bytes = await transform(
      decode_base64url(shared.slice(prefix.length)),
      new DecompressionStream("gzip"),
    );
    bytes[bytes.length - 1] = 0b111 << 2;
    const future =
      prefix +
      encode_base64url(await transform(bytes, new CompressionStream("gzip")));

    const imported = await parse_portable_task(future);
    expect(imported.title).toBe("Future");
    expect(imported.priority).toBeUndefined();
  });

  it("rejects a board export", async () => {
    await expect(parse_portable_task('{"columns":[]}')).rejects.toThrow(
      /supported Cardbe task/,
    );
  });

  it("accepts surrounding whitespace around shared text", async () => {
    const shared = await serialize_portable_task(
      create_task("task_1", "Shared"),
    );
    expect((await parse_portable_task(`  ${shared}\n`)).title).toBe("Shared");
  });

  it("rejects legacy JSON sharing text", async () => {
    const legacy =
      'cardbe-task:{"format":"cardbe-task","version":1,"task":{"id":-1,"title":"Legacy","description":"","color":"","labels":[]}}';
    await expect(parse_portable_task(legacy)).rejects.toThrow(
      /supported Cardbe task/,
    );
  });
});
