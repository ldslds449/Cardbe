import { invoke } from "@tauri-apps/api/core";
import { get_task_id } from "../../type/task.svelte";

/** Keep aligned with `EventOrigin` in `src-tauri/src/task_events.rs`. */
export type TaskEventOrigin = "local" | "remote" | "plugin" | "undo";

/** Keep aligned with `EventKind` in `src-tauri/src/task_events.rs`. */
export const TASK_EVENT_KINDS = [
  "created",
  "moved",
  "archived",
  "restored",
  "deleted",
  "transferred_in",
  "transferred_out",
  "board_reset",
] as const;
export type TaskEventKind = (typeof TASK_EVENT_KINDS)[number];

export interface TaskEvent {
  id: number;
  op_id: number;
  origin: TaskEventOrigin;
  plugin_instance_id: string | null;
  reverts_op_id: number | null;
  at: Date;
  task_id: string;
  kind: TaskEventKind;
  from_column_id: string | null;
  to_column_id: string | null;
}

export interface TaskEventSerialized {
  id: number;
  op_id: number;
  origin: string;
  plugin_instance_id: string | null;
  reverts_op_id: number | null;
  at: number;
  task_id: number;
  kind: string;
  from_column_id: number | null;
  to_column_id: number | null;
}

/** Returns `null` for kinds or origins from a newer version. */
export function deserialize_task_event(
  data: TaskEventSerialized,
): TaskEvent | null {
  if (
    !(TASK_EVENT_KINDS as readonly string[]).includes(data.kind) ||
    !["local", "remote", "plugin", "undo"].includes(data.origin)
  ) {
    return null;
  }
  const column = (id: number | null) => (id === null ? null : `column_${id}`);
  return {
    ...data,
    origin: data.origin as TaskEventOrigin,
    kind: data.kind as TaskEventKind,
    at: new Date(data.at),
    task_id: `task_${data.task_id}`,
    from_column_id: column(data.from_column_id),
    to_column_id: column(data.to_column_id),
  };
}

/**
 * The device-local task event log of the active board, oldest first. With
 * `taskId`, only that task's events and board resets are loaded; with
 * `afterId`, only board events recorded after that event.
 */
export async function load_task_events(
  expectedBoardId: number,
  { taskId, afterId }: { taskId?: string; afterId?: number } = {},
): Promise<TaskEvent[]> {
  const data = await invoke<TaskEventSerialized[]>("get_task_events", {
    expectedBoardId,
    taskId: taskId === undefined ? null : get_task_id(taskId),
    afterId: afterId ?? null,
  });
  return data.flatMap((event) => deserialize_task_event(event) ?? []);
}

/**
 * Events still in effect for one task, oldest first: those after the last
 * board reset, without undone operations or the undos themselves.
 */
export function task_history(events: TaskEvent[], task_id: string) {
  const reset = events.findLastIndex((event) => event.kind === "board_reset");
  const current = events.slice(reset + 1);
  const reverted = new Set(
    current.flatMap((event) =>
      event.origin === "undo" && event.reverts_op_id !== null
        ? [event.reverts_op_id]
        : [],
    ),
  );
  const entries = current.filter(
    (event) =>
      event.task_id === task_id &&
      event.origin !== "undo" &&
      !reverted.has(event.op_id),
  );
  return {
    entries,
    // Without its arrival, the task existed before recording started.
    complete:
      entries.length > 0 &&
      ["created", "transferred_in"].includes(entries[0].kind),
  };
}
