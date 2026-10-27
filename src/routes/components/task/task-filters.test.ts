import { expect, it } from "vite-plus/test";
import {
  default_task_filter_values,
  sort_options,
  task_filter_query,
  TaskFilters,
} from "./task-filters.svelte";

const now = new Date(2026, 9, 27, 15);
const today = new Date(2026, 9, 27).getTime();
const tomorrow = new Date(2026, 9, 28).getTime();

it("sorts searches by relevance until another order is picked", () => {
  const values = { ...default_task_filter_values(), search_text: " report " };
  expect(task_filter_query(values, now)).toMatchObject({
    query: "report",
    sort: "relevance",
  });
  expect(
    task_filter_query({ ...values, prefer_relevance: false }, now).sort,
  ).toBe("due");
  expect(task_filter_query({ ...values, search_text: "" }, now).sort).toBe(
    "due",
  );
  expect(sort_options("active", true)[0].id).toBe("relevance");
  expect(
    sort_options("active", false).map((option) => option.id),
  ).not.toContain("relevance");
});

it("turns relative due and archive filters into local day bounds", () => {
  const values = {
    ...default_task_filter_values(),
    status: "all" as const,
    due: "today" as const,
    archive_time: "7d" as const,
    priorities: ["urgent" as const],
  };
  expect(task_filter_query(values, now)).toMatchObject({
    due: "due",
    due_start: today,
    due_end: tomorrow,
    archive_start: new Date(2026, 9, 21).getTime(),
    archive_end: tomorrow,
    priorities: ["urgent"],
  });
  expect(
    task_filter_query({ ...values, due: "none", archive_time: "all" }, now),
  ).toMatchObject({
    due: "none",
    due_start: null,
    due_end: null,
    archive_start: null,
    archive_end: null,
  });
});

it("applies the column filter only to a single board outside the archive", () => {
  const values = {
    ...default_task_filter_values(),
    board_ids: [3],
    column: "column_5",
  };
  expect(task_filter_query(values, now).column_id).toBe(5);
  expect(
    task_filter_query({ ...values, board_ids: [3, 4] }, now).column_id,
  ).toBeNull();
  expect(
    task_filter_query({ ...values, status: "archived" }, now).column_id,
  ).toBeNull();
});

it("resets dependent conditions when the status, scope, or search changes", () => {
  const filters = new TaskFilters();
  filters.board_ids = [3];
  filters.column = "column_5";
  filters.archive_time = "30d";
  filters.set_status("archived");
  expect(filters.column).toBe("all");
  expect(filters.sort).toBe("archived");
  filters.set_status("active");
  expect(filters.archive_time).toBe("all");
  expect(filters.sort).toBe("due");

  filters.column = "column_5";
  filters.set_board_ids([3, 4]);
  expect(filters.column).toBe("all");

  filters.search_text = "plan";
  filters.set_sort("title");
  expect(filters.effective_sort).toBe("title");
  filters.search_text = "";
  filters.search_text = "plan";
  expect(filters.effective_sort).toBe("relevance");
});

it("copies conditions without sharing their lists", () => {
  const source = new TaskFilters();
  source.board_ids = [1, 2];
  source.priorities = ["high"];
  source.search_text = "x";
  const copy = new TaskFilters();
  copy.assign(source.values);
  source.board_ids.push(3);
  source.priorities.push("low");
  expect(copy.board_ids).toEqual([1, 2]);
  expect(copy.priorities).toEqual(["high"]);
  expect(copy.search_text).toBe("x");
});
