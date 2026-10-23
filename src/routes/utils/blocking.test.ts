import { describe, expect, it } from "vite-plus/test";

import { create_column } from "../type/column.svelte";
import { create_task, type Task } from "../type/task.svelte";
import { can_wait_on, open_blockers, waiting_tasks } from "./blocking";

function task(id: string, blocked_by?: string[]): Task {
  return { ...create_task(id, id), blocked_by };
}

describe("blocking", () => {
  const write = task("task_1");
  const review = task("task_2", ["task_1"]);
  const shipped = task("task_3");
  const deploy = task("task_4", ["task_2", "task_3", "task_9"]);
  const todo = create_column("column_1", "Todo", "", [write, review, deploy]);
  const done = {
    ...create_column("column_2", "Done", "", [shipped]),
    done: true,
  };
  const columns = [todo, done];

  it("counts only open tasks of the board as blockers", () => {
    expect(open_blockers(deploy, columns).map((t) => t.id)).toEqual(["task_2"]);
    expect(open_blockers(write, columns)).toEqual([]);
  });

  it("lists the tasks waiting on a task", () => {
    expect(waiting_tasks(review, columns).map((t) => t.id)).toEqual(["task_4"]);
    expect(waiting_tasks(shipped, columns).map((t) => t.id)).toEqual([
      "task_4",
    ]);
  });

  it("rejects itself, chosen tasks and cycles", () => {
    expect(can_wait_on("task_1", [], "task_3", columns)).toBe(true);
    expect(can_wait_on("task_1", [], "task_1", columns)).toBe(false);
    expect(can_wait_on("task_4", ["task_2"], "task_2", columns)).toBe(false);
    expect(can_wait_on("task_1", [], "task_2", columns)).toBe(false);
    expect(can_wait_on("task_1", [], "task_4", columns)).toBe(false);
    // A new task has no waiting tasks yet.
    expect(can_wait_on("", [], "task_4", columns)).toBe(true);
  });
});
