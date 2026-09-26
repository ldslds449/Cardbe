import { describe, expect, it } from "vitest";

import {
  clone_task,
  create_task_item,
  create_task,
  deserialize_task,
  duplicate_task,
  get_task_id,
  serialize_task,
  set_task_id,
  task_from_template,
  recurrence_label,
} from "./task.svelte";

describe("task serialization", () => {
  it("round-trips dates, labels, and generated ids", () => {
    const checklist_item = create_task_item("Publish release notes");
    checklist_item.completed = true;
    const task = create_task(
      "",
      "Release",
      "Ship the build",
      "#336699",
      new Date("2026-07-16T01:00:00.000Z"),
      new Date("2026-07-17T02:30:00.000Z"),
      ["work", "urgent"],
      [checklist_item],
    );
    set_task_id(task, 42);

    const restored = deserialize_task(serialize_task(task));

    expect(restored).toEqual(task);
    expect(get_task_id(restored)).toBe(42);
  });

  it("clones labels without sharing the mutable array", () => {
    const task = create_task(
      "",
      "Task",
      "",
      "",
      undefined,
      undefined,
      ["one"],
      [create_task_item("First")],
    );
    const cloned = clone_task(task);

    cloned.labels.push("two");
    cloned.items[0].text = "Changed";

    expect(task.labels).toEqual(["one"]);
    expect(cloned.labels).toEqual(["one", "two"]);
    expect(task.items[0].text).toBe("First");
    expect(cloned.items[0].text).toBe("Changed");
  });

  it("duplicates task content with fresh task and checklist ids", () => {
    const item = create_task_item("First");
    item.completed = true;
    const task = create_task(
      "task_42",
      "Task",
      "Description",
      "#336699",
      new Date("2026-07-16T01:00:00.000Z"),
      new Date("2026-07-17T02:30:00.000Z"),
      ["one"],
      [item],
    );

    const duplicate = duplicate_task(task);

    expect(duplicate).toMatchObject({
      ...task,
      id: "",
      items: [{ text: "First", completed: true }],
    });
    expect(duplicate.items[0].id).not.toBe(task.items[0].id);
    expect(duplicate.start_time).not.toBe(task.start_time);
    expect(duplicate.due_time).not.toBe(task.due_time);
    expect(duplicate.labels).not.toBe(task.labels);
  });

  it("defaults missing items from older serialized tasks", () => {
    const restored = deserialize_task({
      id: 7,
      title: "Legacy",
      description: "",
      color: "",
      start_time: undefined,
      due_time: undefined,
      labels: [],
    });

    expect(restored.items).toEqual([]);
  });

  it("creates a fresh task from a template without stale dates or completion", () => {
    const item = create_task_item("Reusable step");
    item.completed = true;
    const template = create_task(
      "",
      "Weekly review",
      "Review the week",
      "#336699",
      new Date("2026-07-16T01:00:00.000Z"),
      new Date("2026-07-17T02:30:00.000Z"),
      ["routine"],
      [item],
    );

    const task = task_from_template(template);

    expect(task.id).toBe("");
    expect(task.start_time).toBeUndefined();
    expect(task.due_time).toBeUndefined();
    expect(task.items[0].completed).toBe(false);
    expect(task.items[0].id).not.toBe(item.id);
  });

  it("rejects malformed task ids", () => {
    expect(() => get_task_id("42")).toThrow("Invalid task ID: 42");
  });

  it("round-trips recurrence without sharing its mutable object", () => {
    const task = create_task();
    task.recurrence = { frequency: "weekly", interval: 2 };

    const restored = deserialize_task(serialize_task(task));
    const cloned = clone_task(task);
    cloned.recurrence!.interval = 3;

    expect(restored.recurrence).toEqual({ frequency: "weekly", interval: 2 });
    expect(task.recurrence.interval).toBe(2);
    expect(recurrence_label(task.recurrence)).toBe("Every 2 weeks");
  });
});
