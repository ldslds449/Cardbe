import { describe, expect, it } from "vite-plus/test";

import {
  create_focus,
  focus_from_explorer,
  focus_query,
  is_builtin_focus,
  today_focus,
  TODAY_FOCUS_ID,
  type ExplorerConditions,
  type Focus,
} from "./focus";

function view(filter: Partial<Focus["filter"]>): Focus {
  const base = create_focus("View");
  return { ...base, filter: { ...base.filter, ...filter } };
}

describe("Focuses", () => {
  // Local 2026-03-10 15:30.
  const now = new Date(2026, 2, 10, 15, 30);
  const day = (offset: number) => new Date(2026, 2, 10 + offset).getTime();

  it("turns due presets into local day ranges", () => {
    expect(focus_query(view({ due: "today" }), now)).toMatchObject({
      status: "all",
      due: "due",
      due_start: day(0),
      due_end: day(1),
    });
    expect(focus_query(view({ due: "by_today" }), now)).toMatchObject({
      due: "due",
      due_start: null,
      due_end: day(1),
    });
    expect(focus_query(view({ due: "next_7_days" }), now)).toMatchObject({
      due_start: day(0),
      due_end: day(7),
    });
    expect(focus_query(view({ due: "overdue" }), now)).toMatchObject({
      status: "overdue",
      due_start: null,
      due_end: null,
    });
    expect(focus_query(view({ due: "none" }), now).due).toBe("none");
    expect(focus_query(view({ due: "all" }), now).due).toBe("all");
  });

  it("passes empty lists as no filter", () => {
    const query = focus_query(
      view({ completion: [], priorities: [], labels: [] }),
      now,
    );
    expect(query.completion).toBeNull();
    expect(query.priorities).toBeNull();
    expect(query.labels).toBeNull();
    expect(
      focus_query(
        view({
          board_ids: [3],
          completion: ["open"],
          labels: ["工作"],
          waiting: "waiting",
          recurring: false,
        }),
        now,
      ),
    ).toMatchObject({
      board_ids: [3],
      completion: ["open"],
      labels: ["工作"],
      waiting: "waiting",
      recurring: false,
      now: now.getTime(),
    });
  });

  it("starts new views with open tasks", () => {
    expect(create_focus("New").filter.completion).toEqual(["open"]);
  });

  it("defines Today as overdue and due-today open tasks of every board", () => {
    const today = today_focus();
    expect(today.id).toBe(TODAY_FOCUS_ID);
    expect(is_builtin_focus(today.id)).toBe(true);
    expect(is_builtin_focus(create_focus("Mine").id)).toBe(false);
    expect(today).toMatchObject({ sort: "due", group: "due" });
    expect(focus_query(today, now)).toMatchObject({
      board_ids: null,
      completion: ["open"],
      due: "due",
      due_start: null,
      due_end: day(1),
    });
  });

  describe("from task explorer conditions", () => {
    const conditions = (
      changes: Partial<ExplorerConditions>,
    ): ExplorerConditions => ({
      query: "",
      board_ids: null,
      status: "active",
      due: "all",
      priorities: [],
      sort: "due",
      other_conditions: false,
      ...changes,
    });

    it("keeps the query, boards, priorities and sort as a new view", () => {
      const board_ids = [2, 5];
      const { view, partial } = focus_from_explorer(
        conditions({
          query: "  報告 ",
          board_ids,
          priorities: ["high"],
          sort: "priority",
        }),
      );
      expect(partial).toBe(false);
      expect(view.name).toBe("");
      expect(view.sort).toBe("priority");
      expect(view.filter).toMatchObject({
        query: "報告",
        board_ids: [2, 5],
        completion: ["open", "done"],
        priorities: ["high"],
        due: "all",
        recurring: null,
      });
      expect(view.filter.board_ids).not.toBe(board_ids);
    });

    it.each([
      ["all", [], "all", null],
      ["archived", ["archived"], "all", null],
      ["overdue", [], "overdue", null],
      ["recurring", ["open", "done"], "all", true],
    ] as const)("maps the %s status", (status, completion, due, recurring) => {
      const { view } = focus_from_explorer(conditions({ status }));
      expect(view.filter).toMatchObject({ completion, due, recurring });
    });

    it.each([
      [{ due: "today" }, false],
      [{ due: "none" }, false],
      [{ due: "due" }, true],
      [{ due: "custom" }, true],
      [{ sort: "column" }, true],
      [{ other_conditions: true }, true],
      [{ status: "overdue", due: "today" }, true],
    ] as const)("reports conditions left out: %o", (changes, partial) => {
      expect(focus_from_explorer(conditions(changes)).partial).toBe(partial);
    });
  });
});
