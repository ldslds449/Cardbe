import { describe, expect, it } from "vite-plus/test";

import {
  create_focus,
  focus_query,
  starter_focuses,
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

  it("starts with open tasks and offers distinct starter views", () => {
    expect(create_focus("New").filter.completion).toEqual(["open"]);
    const starters = starter_focuses();
    expect(starters).toHaveLength(3);
    expect(new Set(starters.map((starter) => starter.id)).size).toBe(3);
    expect(starters[2].filter.waiting).toBe("waiting");
  });
});
