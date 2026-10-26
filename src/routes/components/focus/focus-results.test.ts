import { beforeEach, expect, it, vi } from "vite-plus/test";
import { invoke } from "@tauri-apps/api/core";
import { create_focus } from "../../type/focus";
import { load_focus_results } from "./focus-results";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue({ items: [], next_cursor: null });
});

it("sends combined picker filters to SQLite on every page", async () => {
  const view = create_focus("");
  view.filter.query = "report";
  view.filter.board_ids = [2, 7];
  view.filter.priorities = ["urgent", "none"];
  view.filter.due = "today";
  const now = new Date(2026, 9, 26, 12);
  const filter = {
    query: "report",
    board_ids: [2, 7],
    priorities: ["urgent", "none"],
    completion: ["open"],
    due_start: new Date(2026, 9, 26).getTime(),
    due_end: new Date(2026, 9, 27).getTime(),
  };
  await load_focus_results(view, now, null, 50);
  await load_focus_results(view, now, "50", 50);
  expect(invoke).toHaveBeenNthCalledWith(1, "list_all_tasks", {
    cursor: null,
    limit: 50,
    filter: expect.objectContaining(filter),
  });
  expect(invoke).toHaveBeenNthCalledWith(2, "list_all_tasks", {
    cursor: "50",
    limit: 50,
    filter: expect.objectContaining(filter),
  });
});

it("clears picker restrictions while keeping completed and archived tasks out", async () => {
  await load_focus_results(create_focus(""), new Date(), null, 50);
  expect(invoke).toHaveBeenCalledWith("list_all_tasks", {
    cursor: null,
    limit: 50,
    filter: expect.objectContaining({
      query: "",
      board_ids: null,
      priorities: null,
      completion: ["open"],
      due: "all",
      due_start: null,
      due_end: null,
    }),
  });
});
