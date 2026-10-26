import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { ScriptTarget, transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";
import { parseCommandError } from "#lib/command-errors.js";
import { IrohDeviceStatus, type IrohInvite } from "../../type/iroh-share";

const source = readFileSync(
  new URL("./iroh_share_dialog.svelte", import.meta.url),
  "utf8",
);
function component_functions(...names: string[]) {
  return parse(source, { modern: true })
    .instance!.content.body.filter(
      (node) =>
        node.type === "FunctionDeclaration" && names.includes(node.id!.name),
    )
    .map((node) => {
      const { start, end } = node as typeof node & {
        start: number;
        end: number;
      };
      return source.slice(start, end);
    })
    .join("\n");
}

it("prefills an external invitation without joining and clears unrelated prompts", () => {
  const start = source.lastIndexOf(
    "$effect",
    source.indexOf("if (open && initial_ticket)"),
  );
  const effect = source.slice(
    start,
    source.indexOf("  function open_join()", start),
  );
  const board = { iroh_last_error: { code: "INTERNAL_ERROR" } };
  const state = new Function(
    "$effect",
    "untrack",
    "board",
    transpile(
      `
    let open=true, initial_ticket="cardbe://share/fixture", ticket="old", flow="manage";
    let wait_generation=0, received_waiting_id=7, waiting_device_id="old-device";
    let approval_open=true, revoke_open=true, delete_confirm_open=true;
    ${component_functions("open_join")}
    ${effect}
    return {initial_ticket, ticket, flow, received_waiting_id, waiting_device_id, approval_open, revoke_open, delete_confirm_open, wait_generation};
  `,
      { target: ScriptTarget.ESNext },
    ),
  )(
    (fn: () => void) => fn(),
    (fn: () => void) => fn(),
    board,
  );
  expect(state).toEqual({
    initial_ticket: "",
    ticket: "cardbe://share/fixture",
    flow: "join",
    received_waiting_id: null,
    waiting_device_id: null,
    approval_open: false,
    revoke_open: false,
    delete_confirm_open: false,
    wait_generation: 1,
  });
  expect(board.iroh_last_error).toBe(null);
});

it("returns from board details to the list without closing sharing", async () => {
  const navigation = component_functions(
    "open_managed_board",
    "back_to_overview",
  );
  const heading = { focus: vi.fn() };
  const list = { focus: vi.fn() };
  const viewport = { scrollTo: vi.fn() };
  const ui = new Function(
    "tick",
    "detail_heading",
    "manage_view_button",
    "manage_viewport",
    transpile(
      `
      let open=true, managed_board_id=null, visible_qr_id="fixture-qr";
      ${navigation}
      return {open_managed_board, back_to_overview, state:()=>({open, managed_board_id, visible_qr_id})};
    `,
      { target: ScriptTarget.ESNext },
    ),
  )(async () => {}, heading, list, viewport);
  await ui.open_managed_board(7);
  expect(ui.state()).toEqual({
    open: true,
    managed_board_id: 7,
    visible_qr_id: null,
  });
  expect(heading.focus).toHaveBeenCalled();
  await ui.back_to_overview();
  expect(ui.state()).toEqual({
    open: true,
    managed_board_id: null,
    visible_qr_id: null,
  });
  expect(list.focus).toHaveBeenCalled();
  expect(viewport.scrollTo).toHaveBeenNthCalledWith(1, { top: 0 });
  expect(viewport.scrollTo).toHaveBeenNthCalledWith(2, { top: 0 });
});

it("returns invitation creation to its origin even when the form selects another board", async () => {
  const heading = { focus: vi.fn() };
  const list = { focus: vi.fn() };
  const viewport = { scrollTo: vi.fn() };
  const invalidate = vi.fn();
  const ui = new Function(
    "tick",
    "invalidate",
    "detail_heading",
    "manage_view_button",
    "manage_viewport",
    transpile(
      `
    let flow="manage", selected_board_id=1, managed_board={board_id:7}, share_return_to_manage=false;
    ${component_functions("open_share", "leave_share")}
    return { open_share, leave_share,
      select_board:()=>selected_board_id=99,
      remove_board:()=>{managed_board=null; detail_heading=null;},
      home:()=>{flow="home"; managed_board=null;},
      state:()=>({flow, selected_board_id, share_return_to_manage, managed_board}) };
  `,
      { target: ScriptTarget.ESNext },
    ),
  )(async () => {}, invalidate, heading, list, viewport);
  ui.open_share();
  expect(ui.state()).toMatchObject({ flow: "share", selected_board_id: 7 });
  ui.select_board();
  await ui.leave_share();
  expect(ui.state()).toMatchObject({
    flow: "manage",
    managed_board: { board_id: 7 },
  });
  expect(heading.focus).toHaveBeenCalled();
  ui.open_share();
  ui.remove_board();
  await ui.leave_share();
  expect(list.focus).toHaveBeenCalled();
  ui.home();
  ui.open_share();
  await ui.leave_share();
  expect(ui.state()).toMatchObject({
    flow: "home",
    share_return_to_manage: false,
  });
  expect(invalidate).toHaveBeenCalledTimes(6);
});

it("restores review focus to the trigger, or a visible heading or list after approval removes it", async () => {
  const trigger = { isConnected: true, focus: vi.fn() };
  const heading = { isConnected: true, focus: vi.fn() };
  const list = { focus: vi.fn() };
  const invite = { invite_id: "fixture-invite" };
  const ui = new Function(
    "tick",
    "detail_heading",
    "manage_view_button",
    transpile(
      `
    let approval_trigger=null, approval_candidate=null, device_id_verified=true, approval_open=false;
    ${component_functions("open_device_review", "restore_review_focus")}
    return {open_device_review, restore_review_focus,
      state:()=>({approval_candidate, device_id_verified, approval_open, approval_trigger})};
  `,
      { target: ScriptTarget.ESNext },
    ),
  )(async () => {}, heading, list);
  ui.open_device_review(invite, "fixture-device", trigger);
  expect(ui.state()).toMatchObject({
    approval_candidate: { invite, node_id: "fixture-device" },
    device_id_verified: false,
    approval_open: true,
  });
  await ui.restore_review_focus();
  expect(trigger.focus).toHaveBeenCalledTimes(1);
  expect(ui.state().approval_trigger).toBeNull();
  trigger.isConnected = false;
  ui.open_device_review(invite, "fixture-device", trigger);
  await ui.restore_review_focus();
  expect(heading.focus).toHaveBeenCalledTimes(1);
  heading.isConnected = false;
  ui.open_device_review(invite, "fixture-device", trigger);
  await ui.restore_review_focus();
  expect(list.focus).toHaveBeenCalledTimes(1);
  expect(trigger.focus).toHaveBeenCalledTimes(1);
});

it("groups invitations by board identity while preserving device permission scopes", () => {
  const group = new Function(
    "IrohDeviceStatus",
    transpile(
      `${component_functions("group_invites")}; return group_invites;`,
      {
        target: ScriptTarget.ESNext,
      },
    ),
  )(IrohDeviceStatus) as (items: IrohInvite[]) => Array<{
    board_id: number;
    invites: IrohInvite[];
    approved: number;
  }>;
  const invite: IrohInvite = {
    invite_id: "viewer",
    board_id: 1,
    board_name: "Same name",
    permission: "viewer",
    enabled: true,
    created_at: "2026-10-13",
    devices: [{ node_id: "one", status: IrohDeviceStatus.Approved }],
  };
  const editor: IrohInvite = {
    ...invite,
    invite_id: "editor",
    permission: "editor",
    devices: [
      { node_id: "two", status: IrohDeviceStatus.Pending },
      { node_id: "three", status: IrohDeviceStatus.Revoked },
    ],
  };
  const other = { ...invite, invite_id: "other", board_id: 2 };
  expect(group([])).toEqual([]);
  expect(group([invite, editor, other])).toEqual([
    {
      board_id: 1,
      board_name: "Same name",
      invites: [invite, editor],
      approved: 1,
    },
    { board_id: 2, board_name: "Same name", invites: [other], approved: 1 },
  ]);
  expect(invite.devices).toHaveLength(1);
  expect(editor.permission).toBe("editor");
});

function creation_ui(
  createInvite: ReturnType<typeof vi.fn>,
  qr = vi.fn().mockResolvedValue("<svg/>"),
) {
  const clipboard = vi.fn().mockResolvedValue(undefined);
  const success = vi.fn();
  const error = vi.fn();
  const load = vi.fn().mockResolvedValue(undefined);
  const ui = new Function(
    "board",
    "parseCommandError",
    "translateCommandError",
    "toast",
    "logger",
    "invoke",
    "navigator",
    "m",
    "load",
    transpile(
      `
      let selected_board_id=7,permission="viewer",selected_board={name:"Board A"};
      let creating=false,create_error=null,generated=null;
      let create_generation=0,qr_loading=false,qr_error=null,open=true,flow="share";
      ${component_functions("create", "invalidate", "generate_qr", "copy")}
      return {create,invalidate,generate_qr,copy,
        change:()=>{selected_board_id=8;permission="editor";selected_board={name:"Board B"};},
        close:()=>{open=false;invalidate();},
        leave:()=>{flow="manage";invalidate();},
        reopen:()=>{open=true;flow="share";invalidate();},
        state:()=>({creating,create_error,generated,qr_error,qr_loading})};
    `,
      { target: ScriptTarget.ESNext },
    ),
  )(
    { create_iroh_invite: createInvite },
    parseCommandError,
    vi.fn(),
    { error, success },
    { error: vi.fn(), warn: vi.fn() },
    qr,
    { clipboard: { writeText: clipboard } },
    {
      ui_invitation_copied: vi.fn(),
      ui_invitation_created: vi.fn(),
      board_title: () => "Board",
    },
    load,
  );
  return { ui, qr, clipboard, success, error, load };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

it("keeps creation failures inline, clears them on edits and recovers on retry", async () => {
  const createInvite = vi
    .fn()
    .mockRejectedValueOnce({ code: "IROH_DISABLED" })
    .mockRejectedValueOnce("private internal exception")
    .mockResolvedValue("fixture-ticket");
  const { ui } = creation_ui(createInvite);
  await ui.create();
  expect(ui.state().create_error).toEqual({ code: "IROH_DISABLED" });
  expect(ui.state().creating).toBe(false);
  ui.invalidate();
  expect(ui.state().create_error).toBeNull();
  await ui.create();
  expect(ui.state().create_error).toEqual({ code: "INTERNAL_ERROR" });
  await ui.create();
  expect(ui.state().create_error).toBeNull();
  expect(ui.state().generated.ticket).toBe("fixture-ticket");
});

it("interprets SQLite invitation timestamps as UTC before local formatting", () => {
  const source = readFileSync(
    new URL("../share/iroh_invite_card.svelte", import.meta.url),
    "utf8",
  );
  const node = parse(source, { modern: true }).instance!.content.body.find(
    (node) =>
      node.type === "FunctionDeclaration" &&
      node.id!.name === "invitation_date",
  )!;
  const { start, end } = node as typeof node & { start: number; end: number };
  const format = new Function(
    "m",
    "sync_time",
    transpile(`${source.slice(start, end)}; return invitation_date;`),
  )(
    {
      share_creation_date_unavailable: () => "unavailable",
      share_created_date: ({ date }: { date: string }) => date,
    },
    new Intl.DateTimeFormat("zh-TW", {
      timeZone: "Asia/Taipei",
      dateStyle: "medium",
      timeStyle: "short",
    }),
  );
  const expected = new Intl.DateTimeFormat("zh-TW", {
    timeZone: "Asia/Taipei",
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date("2026-10-12T18:30:00Z"));
  expect(format("2026-10-12 18:30:00")).toBe(expected);
  expect(format("2026-10-12T18:30:00Z")).toBe(expected);
  expect(format("2026-10-13T02:30:00+08:00")).toBe(expected);
  expect(format("invalid")).toBe("unavailable");
});

it("captures the submitted board and permission and prevents overlapping creation", async () => {
  const pending = deferred<string>();
  const createInvite = vi.fn().mockReturnValue(pending.promise);
  const { ui } = creation_ui(createInvite);
  const creating = ui.create();
  ui.change();
  await ui.create();
  expect(createInvite).toHaveBeenCalledExactlyOnceWith(7, "viewer");
  pending.resolve("ticket-A-viewer");
  await creating;
  expect(ui.state().generated).toMatchObject({
    ticket: "ticket-A-viewer",
    board_name: "Board A",
    permission: "viewer",
  });
});

it.each(["close", "leave"])(
  "ignores a completed creation after %s and reopening",
  async (action) => {
    const pending = deferred<string>();
    const { ui, clipboard, qr, load, success } = creation_ui(
      vi.fn().mockReturnValue(pending.promise),
    );
    const creating = ui.create();
    ui[action]();
    ui.reopen();
    pending.resolve("old-ticket");
    await creating;
    expect(ui.state()).toMatchObject({
      generated: null,
      create_error: null,
      creating: false,
    });
    expect(clipboard).not.toHaveBeenCalled();
    expect(qr).not.toHaveBeenCalled();
    expect(load).not.toHaveBeenCalled();
    expect(success).not.toHaveBeenCalled();
  },
);

it("does not show an old creation failure in a reopened form", async () => {
  const pending = deferred<string>();
  const { ui, error } = creation_ui(vi.fn().mockReturnValue(pending.promise));
  const creating = ui.create();
  ui.close();
  ui.reopen();
  pending.reject({ code: "IROH_DISABLED" });
  await creating;
  expect(ui.state().create_error).toBeNull();
  expect(error).not.toHaveBeenCalled();
});

it("keeps a copyable invitation when QR generation fails and retries only its QR", async () => {
  const createInvite = vi.fn().mockResolvedValue("saved-ticket");
  const qr = vi
    .fn()
    .mockRejectedValueOnce({ code: "INTERNAL_ERROR" })
    .mockResolvedValue("<svg/>");
  const { ui, clipboard, error, success } = creation_ui(createInvite, qr);
  await ui.create();
  expect(ui.state()).toMatchObject({
    generated: { ticket: "saved-ticket", qr: "" },
    create_error: null,
    qr_error: { code: "INTERNAL_ERROR" },
    creating: false,
    qr_loading: false,
  });
  expect(success).toHaveBeenCalledTimes(1);
  expect(error).not.toHaveBeenCalled();
  await ui.copy();
  expect(clipboard).toHaveBeenLastCalledWith("saved-ticket");
  await ui.generate_qr();
  expect(createInvite).toHaveBeenCalledTimes(1);
  expect(qr.mock.calls).toEqual([
    ["iroh_invite_qr_svg", { ticket: "saved-ticket" }],
    ["iroh_invite_qr_svg", { ticket: "saved-ticket" }],
  ]);
  expect(ui.state()).toMatchObject({
    generated: { ticket: "saved-ticket", qr: "<svg/>" },
    qr_error: null,
  });
});

it("makes the created link usable before QR generation finishes", async () => {
  const pending = deferred<string>();
  const qr = vi.fn().mockReturnValue(pending.promise);
  const { ui, clipboard } = creation_ui(
    vi.fn().mockResolvedValue("saved-ticket"),
    qr,
  );
  await ui.create();
  expect(ui.state()).toMatchObject({
    creating: false,
    qr_loading: true,
    generated: { ticket: "saved-ticket", qr: "" },
  });
  await ui.copy();
  expect(clipboard).toHaveBeenLastCalledWith("saved-ticket");
  await ui.generate_qr();
  expect(qr).toHaveBeenCalledTimes(1);
  pending.resolve("<svg/>");
  await Promise.resolve();
  expect(ui.state()).toMatchObject({
    qr_loading: false,
    generated: { ticket: "saved-ticket", qr: "<svg/>" },
  });
});

it.each(["resolve", "reject"])(
  "ignores an old QR retry that later %s",
  async (outcome) => {
    const pending = deferred<string>();
    const qr = vi
      .fn()
      .mockRejectedValueOnce({ code: "INTERNAL_ERROR" })
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValue("<svg-new/>");
    const createInvite = vi
      .fn()
      .mockResolvedValueOnce("old-ticket")
      .mockResolvedValueOnce("new-ticket");
    const { ui } = creation_ui(createInvite, qr);
    await ui.create();
    const retry = ui.generate_qr();
    ui.change();
    ui.invalidate();
    await ui.create();
    if (outcome === "resolve") {
      pending.resolve("<svg-old/>");
    } else {
      pending.reject({ code: "INTERNAL_ERROR" });
    }
    await retry;
    expect(ui.state()).toMatchObject({
      generated: {
        ticket: "new-ticket",
        qr: "<svg-new/>",
        board_name: "Board B",
        permission: "editor",
      },
      qr_error: null,
      qr_loading: false,
    });
  },
);
