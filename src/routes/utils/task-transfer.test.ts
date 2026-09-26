import { describe, expect, it } from "vitest";

import { create_task, create_task_item } from "../type/task.svelte";
import { parse_portable_task, serialize_portable_task } from "./task-transfer";

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
    expect(imported.due_time?.getTime()).toBe(2000);
    expect(imported.labels).toEqual(["work"]);
    expect(imported.recurrence).toEqual({ frequency: "weekly", interval: 2 });
    expect(imported.items[0].text).toBe("Ship it");
    expect(imported.items[0].id).not.toBe(item.id);
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
