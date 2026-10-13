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
