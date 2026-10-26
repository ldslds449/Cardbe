import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import { translateCommandError } from "#lib/command-errors.js";
import * as m from "#lib/paraglide/messages.js";

export type DeepLink =
  | { kind: "invite"; ticket: string }
  | { kind: "board"; reference: string; task_id: number | null }
  | { kind: "invalid" };

export async function copyBoardLink(boardId: number, taskId?: number) {
  try {
    const link = await invoke<string>("create_board_link", {
      boardId,
      taskId: taskId ?? null,
    });
    try {
      await navigator.clipboard.writeText(link);
      toast.success(m.deep_link_copied());
    } catch {
      toast.error(m.deep_link_copy_failed());
    }
  } catch (error) {
    toast.error(translateCommandError(error));
  }
}
