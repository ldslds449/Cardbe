import type { TextTarget } from "../../utils/markdown-outline";

// Hands the double-clicked description position from the details dialog to
// the editor dialog that the details dialog's Edit callback opens next.
let pending: { task_id: string; target: TextTarget } | null = null;

export function request_description_reveal(
  task_id: string,
  target: TextTarget,
) {
  pending = { task_id, target };
}

export function take_description_reveal(
  task_id: string,
): TextTarget | undefined {
  const request = pending;
  pending = null;
  return request?.task_id === task_id ? request.target : undefined;
}
