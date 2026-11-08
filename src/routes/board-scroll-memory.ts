// Scroll positions remembered per board and view for this session, so
// switching boards returns each one to where it was left. Each view decides
// the units: the board stores pixels, the Gantt chart its center day.
export type ScrollMemoryView = "board" | "gantt";

export interface ScrollPosition {
  x: number;
  y: number;
}

const positions = new Map<string, ScrollPosition>();

function key(board_id: number, view: ScrollMemoryView): string {
  return `${board_id}:${view}`;
}

export function remembered_scroll(
  board_id: number,
  view: ScrollMemoryView,
): ScrollPosition | undefined {
  return positions.get(key(board_id, view));
}

export function remember_scroll(
  board_id: number,
  view: ScrollMemoryView,
  position: ScrollPosition,
) {
  positions.set(key(board_id, view), position);
}
