import { readFileSync } from "node:fs";
import { parse } from "svelte/compiler";
import { transpile } from "typescript";
import { expect, it, vi } from "vite-plus/test";
import { parseCommandError } from "$lib/command-errors";

const source = readFileSync(
  new URL("./iroh_share_dialog.svelte", import.meta.url),
  "utf8",
);
const functions = parse(source, { modern: true })
  .instance!.content.body.filter(
    (node) =>
      node.type === "FunctionDeclaration" &&
      ["create", "invalidate"].includes(node.id!.name),
  )
  .map((node) => {
    const { start, end } = node as typeof node & { start: number; end: number };
    return source.slice(start, end);
  })
  .join("\n");

it("keeps creation failures inline, clears them on edits and recovers on retry", async () => {
  const createInvite = vi
    .fn()
    .mockRejectedValueOnce({ code: "IROH_DISABLED" })
    .mockRejectedValueOnce("private internal exception")
    .mockResolvedValue("fixture-ticket");
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
    transpile(`
      let selected_board_id=7,permission="viewer",selected_board={name:"Board"};
      let creating=false,create_error=null,generated=null;
      ${functions}
      return {create,invalidate,state:()=>({creating,create_error,generated})};
    `),
  )(
    { create_iroh_invite: createInvite },
    parseCommandError,
    vi.fn(),
    { error: vi.fn(), success: vi.fn() },
    { error: vi.fn(), warn: vi.fn() },
    vi.fn().mockResolvedValue("<svg/>"),
    { clipboard: { writeText: vi.fn() } },
    { ui_invitation_copied: vi.fn(), ui_invitation_created: vi.fn() },
    vi.fn(),
  );
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
