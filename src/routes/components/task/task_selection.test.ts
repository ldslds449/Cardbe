import { describe, expect, it } from "vite-plus/test";
import { create_column } from "../../type/column.svelte";
import { create_task } from "../../type/task.svelte";
import {
  filter_selection_columns,
  select_column_tasks,
  select_one_task,
  select_visible_tasks,
} from "./task_selection";

const first = create_task("task_1", "中文任務");
first.description = "details";
first.labels = ["重要"];
const columns = [
  create_column("column_1", "Ready", "", [
    first,
    create_task("task_2", "Other"),
  ]),
  create_column("column_2", "Later", "", [create_task("task_3", "Third")]),
  create_column("column_3", "Empty"),
];
const empty = { column_ids: [] as string[], task_ids: [] as string[] };

describe("shared task picker", () => {
  it("matches column names, titles, descriptions, and labels without changing source data", () => {
    for (const query of ["中文", "DETAILS", "重要"]) {
      expect(
        filter_selection_columns(columns, query)[0].tasks.map(
          (task) => task.id,
        ),
      ).toEqual(["task_1"]);
    }
    expect(filter_selection_columns(columns, "ready")[0].tasks).toHaveLength(2);
    expect(filter_selection_columns(columns, "missing")).toEqual([]);
    expect(filter_selection_columns(columns, " ")).toBe(columns);
    expect(columns[0].tasks).toHaveLength(2);
  });

  it("selects complete columns, including empty ones, and preserves other column selections", () => {
    const selected = select_column_tasks(columns, empty, "column_1", true);
    expect(selected.task_ids).toEqual(["task_1", "task_2"]);
    expect(select_column_tasks(columns, selected, "column_1", true)).toEqual(
      selected,
    );
    const both = select_column_tasks(columns, selected, "column_2", true);
    expect(select_column_tasks(columns, both, "column_1", false)).toEqual({
      column_ids: ["column_2"],
      task_ids: ["task_3"],
    });
    expect(select_column_tasks(columns, empty, "column_3", true)).toEqual({
      column_ids: ["column_3"],
      task_ids: [],
    });
  });

  it("keeps publisher column membership when a single task is unchecked", () => {
    const selected = select_one_task(empty, "column_1", "task_1", true);
    expect(select_one_task(selected, "column_1", "task_1", true)).toEqual(
      selected,
    );
    expect(select_one_task(selected, "column_1", "task_1", false)).toEqual({
      column_ids: ["column_1"],
      task_ids: [],
    });
  });

  it("selects and clears search results while preserving hidden tasks and their columns", () => {
    const selected = select_column_tasks(
      columns,
      select_column_tasks(columns, empty, "column_1", true),
      "column_2",
      true,
    );
    const visible = filter_selection_columns(columns, "中文");
    const cleared = select_visible_tasks(columns, visible, selected, false);
    expect(cleared.task_ids).toEqual(["task_2", "task_3"]);
    expect(cleared.column_ids).toEqual(["column_1", "column_2"]);
    expect(
      select_visible_tasks(columns, visible, cleared, true).task_ids,
    ).toEqual(["task_2", "task_3", "task_1"]);
    expect(select_visible_tasks(columns, columns, selected, false)).toEqual(
      empty,
    );
  });
});
