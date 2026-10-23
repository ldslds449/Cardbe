import { describe, expect, it } from "vite-plus/test";

import {
  create_smart_view,
  smart_view_query,
  starter_smart_views,
  type SmartView,
} from "./smart-view";

function view(filter: Partial<SmartView["filter"]>): SmartView {
  const base = create_smart_view("View");
  return { ...base, filter: { ...base.filter, ...filter } };
}

describe("smart views", () => {
  // Local 2026-03-10 15:30.
  const now = new Date(2026, 2, 10, 15, 30);
  const day = (offset: number) => new Date(2026, 2, 10 + offset).getTime();

  it("turns due presets into local day ranges", () => {
    expect(smart_view_query(view({ due: "today" }), now)).toMatchObject({
      status: "all",
      due: "due",
      due_start: day(0),
      due_end: day(1),
    });
    expect(smart_view_query(view({ due: "by_today" }), now)).toMatchObject({
      due: "due",
      due_start: null,
      due_end: day(1),
    });
    expect(smart_view_query(view({ due: "next_7_days" }), now)).toMatchObject({
      due_start: day(0),
      due_end: day(7),
    });
    expect(smart_view_query(view({ due: "overdue" }), now)).toMatchObject({
      status: "overdue",
      due_start: null,
      due_end: null,
    });
    expect(smart_view_query(view({ due: "none" }), now).due).toBe("none");
    expect(smart_view_query(view({ due: "all" }), now).due).toBe("all");
  });

  it("passes empty lists as no filter", () => {
    const query = smart_view_query(
      view({ completion: [], priorities: [], labels: [] }),
      now,
    );
    expect(query.completion).toBeNull();
    expect(query.priorities).toBeNull();
    expect(query.labels).toBeNull();
    expect(
      smart_view_query(
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
    expect(create_smart_view("New").filter.completion).toEqual(["open"]);
    const starters = starter_smart_views();
    expect(starters).toHaveLength(3);
    expect(new Set(starters.map((starter) => starter.id)).size).toBe(3);
    expect(starters[2].filter.waiting).toBe("waiting");
  });
});
