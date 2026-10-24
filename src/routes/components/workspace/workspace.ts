/** Board views in tab order; Ctrl+1 opens the first. */
export const WORKSPACE_VIEWS = [
  "board",
  "calendar",
  "gantt",
  "flow",
  "statistics",
] as const;

export type WorkspaceView = (typeof WORKSPACE_VIEWS)[number];

/** Shown in place of the board like a Focus, under an ID no Focus uses. */
export const HOME_PAGE_ID = "home";
