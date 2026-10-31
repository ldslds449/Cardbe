import { readFileSync } from "node:fs";
import { transpile, ScriptTarget } from "typescript";
import { expect, it, vi } from "vite-plus/test";

const source = readFileSync(
  new URL("../routes/+page.svelte", import.meta.url),
  "utf8",
);
const handler = source.slice(
  source.indexOf("    async function process_links()"),
  source.indexOf("    const deep_link_listener"),
);
function setup(batches: unknown[][], ready = true) {
  const board = {
    active_board_id: 7,
    get_expired_task_detail: vi.fn().mockResolvedValue(null),
    get_archive_detail: vi.fn().mockResolvedValue(null),
  };
  const toast = { error: vi.fn() };
  const view = vi.fn(),
    archive = vi.fn();
  const invoke = vi.fn(async (command: string) =>
    command === "take_deep_links" ? (batches.shift() ?? []) : 7,
  );
  const ui = new Function(
    "invoke",
    "board",
    "toast",
    "view_task_by_id",
    "view_archived_task",
    "translateCommandError",
    "m",
    transpile(
      `
    let links_ready=${ready}, handling_links=false, links_pending=false, disposed=false;
    let deep_link_ticket="", iroh_show_requests=false, iroh_show_received=false, iroh_share_dialog_open=false, active_focus_id="home", task_dialog_open=false, task_dialog_mode="view";
    const processing=new Set();
    async function switch_board(id) { board.active_board_id=id; }
    ${handler}
    return { process_links, ready:()=>links_ready=true, state:()=>({deep_link_ticket, iroh_share_dialog_open, active_focus_id}) };
  `,
      { target: ScriptTarget.ESNext },
    ),
  )(invoke, board, toast, view, archive, (e: { code: string }) => e.code, {
    task_missing: () => "missing task",
    deep_link_invalid: () => "invalid link",
  });
  return { ui, board, toast, invoke, view, archive };
}

it("keeps cold-start links queued until initialization and only prefills invitations", async () => {
  const { ui, invoke } = setup(
    [[{ kind: "invite", ticket: "cardbe://share/fixture" }]],
    false,
  );
  await ui.process_links();
  expect(invoke).not.toHaveBeenCalled();
  ui.ready();
  await ui.process_links();
  expect(ui.state()).toMatchObject({
    deep_link_ticket: "cardbe://share/fixture",
    iroh_share_dialog_open: true,
  });
  expect(
    invoke.mock.calls.every(([command]) => command === "take_deep_links"),
  ).toBe(true);
});

it("reports a missing board without starting an invitation flow", async () => {
  const { ui, invoke, toast } = setup([
    [{ kind: "board", reference: "missing", task_id: null }],
  ]);
  invoke.mockImplementation(async (command) => {
    if (command === "resolve_board_link") {
      throw { code: "BOARD_NOT_FOUND" };
    }
    return command === "take_deep_links" && invoke.mock.calls.length === 1
      ? [{ kind: "board", reference: "missing", task_id: null }]
      : [];
  });
  await ui.process_links();
  expect(toast.error).toHaveBeenCalledWith("BOARD_NOT_FOUND");
  expect(ui.state().iroh_share_dialog_open).toBe(false);
});

it("opens archived tasks and deduplicates links during processing", async () => {
  const link = { kind: "board", reference: "reference", task_id: 42 };
  const { ui, board, archive, toast } = setup([[link, link]]);
  board.get_archive_detail.mockResolvedValue({ id: "task_42" });
  await ui.process_links();
  expect(archive).toHaveBeenCalledExactlyOnceWith({ id: "task_42" });
  expect(toast.error).not.toHaveBeenCalled();
  expect(ui.state().active_focus_id).toBe(null);
});

it("reports missing tasks and invalid links", async () => {
  const { ui, toast } = setup([
    [
      { kind: "board", reference: "reference", task_id: 42 },
      { kind: "invalid" },
    ],
  ]);
  await ui.process_links();
  expect(toast.error.mock.calls).toEqual([["missing task"], ["invalid link"]]);
});

it("drains an event arriving while an empty queue response is in flight", async () => {
  const { ui, invoke } = setup([]);
  let resolve!: (value: unknown[]) => void;
  invoke.mockImplementationOnce(
    () =>
      new Promise<unknown[]>((done) => {
        resolve = done;
      }),
  );
  const first = ui.process_links();
  await ui.process_links();
  resolve([]);
  await first;
  expect(invoke).toHaveBeenCalledTimes(2);
});
