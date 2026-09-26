import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  build_share_snapshot,
  group_enabled_shares_by_board,
  parse_requested_share_link,
  publish_share,
  share_content_signature,
  type ManagedShare,
} from "./share";

import { create_column } from "./type/column.svelte";
import { create_task } from "./type/task.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("board sharing", () => {
  it("groups enabled shares so each board needs only one column fetch", () => {
    const base = {
      url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
      updated_at: "2026-09-01T12:00:00Z",
      expires_at: null,
      title: "Board",
      selected_column_ids: [],
      selected_task_ids: [],
      enabled: true,
    } satisfies Omit<ManagedShare, "id" | "board_id">;
    const grouped = group_enabled_shares_by_board([
      { ...base, id: "share_a_abcdefghijklmn", board_id: 1 },
      { ...base, id: "share_b_abcdefghijklmn", board_id: 1 },
      { ...base, id: "share_c_abcdefghijklmn", board_id: 2 },
      { ...base, id: "share_d_abcdefghijklmn", board_id: 3, enabled: false },
      { ...base, id: "share_e_abcdefghijklmn" },
    ]);
    expect([...grouped.keys()]).toEqual([1, 2]);
    expect(grouped.get(1)).toHaveLength(2);
    expect(grouped.get(2)).toHaveLength(1);
  });
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("publishes only selected active columns", () => {
    const todo = create_column("column_1", "Todo", "", [
      create_task("task_2", "Ship"),
      create_task("task_5", "Keep private"),
    ]);
    const private_column = create_column("column_3", "Private", "", [
      create_task("task_4", "Secret"),
    ]);
    const snapshot = build_share_snapshot(
      [todo, private_column],
      [todo.id],
      [todo.tasks[0].id],
      " Roadmap ",
      new Date("2026-08-22T12:00:00Z"),
    );

    expect(snapshot.title).toBe("Roadmap");
    expect(snapshot.published_at).toBe("2026-08-22T12:00:00.000Z");
    expect(snapshot.columns).toHaveLength(1);
    expect(snapshot.columns[0].tasks).toHaveLength(1);
    expect(snapshot.columns[0].tasks[0].title).toBe("Ship");
  });

  it("requires a title and at least one selected column", () => {
    const column = create_column("column_1", "Todo");
    expect(() => build_share_snapshot([column], [], [], "Board")).toThrow(
      "Select at least one",
    );
    expect(() => build_share_snapshot([column], [column.id], [], "  ")).toThrow(
      "Enter a name",
    );
  });

  it("updates shared tasks but does not add newly created tasks", () => {
    const column = create_column("column_1", "Todo", "", [
      create_task("task_2", "Draft"),
    ]);
    const selected_columns = [column.id];
    const selected_tasks = [column.tasks[0].id];
    const before = share_content_signature(
      [column],
      selected_columns,
      selected_tasks,
      "Board",
    );

    column.tasks.push(create_task("task_3", "New task"));
    expect(
      share_content_signature(
        [column],
        selected_columns,
        selected_tasks,
        "Board",
      ),
    ).toBe(before);

    column.tasks[0].title = "Published";

    expect(
      share_content_signature(
        [column],
        selected_columns,
        selected_tasks,
        "Board",
      ),
    ).not.toBe(before);
  });

  it("ignores changes to tasks that are not shared", () => {
    const column = create_column("column_1", "Todo", "", [
      create_task("task_2", "Shared"),
      create_task("task_3", "Private"),
    ]);
    const before = share_content_signature(
      [column],
      [column.id],
      [column.tasks[0].id],
      "Board",
    );

    column.tasks[1].title = "Still private";

    expect(
      share_content_signature(
        [column],
        [column.id],
        [column.tasks[0].id],
        "Board",
      ),
    ).toBe(before);
  });

  it("reuses the complete address of an existing share", async () => {
    const snapshot = build_share_snapshot(
      [create_column("column_1", "Todo")],
      ["column_1"],
      [],
      "Roadmap",
    );
    const existing: ManagedShare = {
      id: "j3V_BXrcsGwtsXv6XAD1jA",
      url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
      updated_at: "2026-09-01T12:00:00Z",
      expires_at: null,
      title: "Roadmap",
      selected_column_ids: ["column_1"],
      selected_task_ids: [],
    };
    vi.mocked(invoke).mockResolvedValue({
      id: existing.id,
      url: existing.url,
      updated_at: "2026-09-09T12:00:00Z",
      expires_at: null,
    });

    await publish_share(snapshot, ["column_1"], [], null, existing);

    expect(invoke).toHaveBeenCalledWith(
      "publish_lan_share",
      expect.objectContaining({
        shareId: existing.id,
        allowReactivate: false,
        preferredPort: 12345,
      }),
    );
  });

  it("parses a previous share URL or a raw share ID", () => {
    expect(
      parse_requested_share_link(
        "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
      ),
    ).toEqual({
      id: "j3V_BXrcsGwtsXv6XAD1jA",
      preferred_port: 12345,
    });
    expect(parse_requested_share_link(" j3V_BXrcsGwtsXv6XAD1jA ")).toEqual({
      id: "j3V_BXrcsGwtsXv6XAD1jA",
      preferred_port: null,
    });
  });

  it("rejects malformed or unsafe previous share links", () => {
    expect(() => parse_requested_share_link("short")).toThrow(
      "valid Cardbe share link",
    );
    expect(() =>
      parse_requested_share_link(
        "https://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
      ),
    ).toThrow();
    expect(() =>
      parse_requested_share_link(
        "http://192.168.1.20/share/j3V_BXrcsGwtsXv6XAD1jA",
      ),
    ).toThrow();
    expect(() =>
      parse_requested_share_link(
        "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA?x=1",
      ),
    ).toThrow();
  });

  it("publishes a manually restored share with its previous ID and port", async () => {
    const snapshot = build_share_snapshot(
      [create_column("column_1", "Todo")],
      ["column_1"],
      [],
      "Roadmap",
    );
    vi.mocked(invoke).mockResolvedValue({
      id: "j3V_BXrcsGwtsXv6XAD1jA",
      url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
      updated_at: "2026-09-17T12:00:00Z",
      expires_at: null,
    });

    await publish_share(
      snapshot,
      ["column_1"],
      [],
      null,
      null,
      [],
      "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
    );

    expect(invoke).toHaveBeenCalledWith(
      "publish_lan_share",
      expect.objectContaining({
        shareId: "j3V_BXrcsGwtsXv6XAD1jA",
        allowReactivate: true,
        preferredPort: 12345,
      }),
    );
  });

  it("reactivates a disabled managed share with the same ID and port", async () => {
    const snapshot = build_share_snapshot(
      [create_column("column_1", "Todo")],
      ["column_1"],
      [],
      "Roadmap",
    );
    const existing: ManagedShare = {
      id: "j3V_BXrcsGwtsXv6XAD1jA",
      url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
      updated_at: "2026-09-01T12:00:00Z",
      expires_at: null,
      title: "Roadmap",
      selected_column_ids: ["column_1"],
      selected_task_ids: [],
      enabled: false,
    };
    vi.mocked(invoke).mockResolvedValue({
      id: existing.id,
      url: existing.url,
      updated_at: "2026-09-19T12:00:00Z",
      expires_at: null,
    });

    const enabled = await publish_share(
      snapshot,
      ["column_1"],
      [],
      null,
      existing,
      [],
      null,
      true,
    );

    expect(invoke).toHaveBeenCalledWith(
      "publish_lan_share",
      expect.objectContaining({
        shareId: existing.id,
        allowReactivate: true,
        preferredPort: 12345,
      }),
    );
    expect(enabled.enabled).toBe(true);
  });
});
