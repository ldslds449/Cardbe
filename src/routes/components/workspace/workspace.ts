/** Board views in tab order; Ctrl+1 opens the first. */
export const WORKSPACE_VIEWS = [
  "board",
  "calendar",
  "gantt",
  "flow",
  "statistics",
] as const;

export type WorkspaceView = (typeof WORKSPACE_VIEWS)[number];
