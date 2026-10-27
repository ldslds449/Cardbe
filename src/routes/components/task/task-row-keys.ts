const KEYS = ["ArrowDown", "ArrowUp", "Home", "End"];

/**
 * Moves focus between the task rows inside the element handling the event,
 * across groups. Only rendered rows take part, so a virtual list moves within
 * the rows it has drawn and scrolls as focus reaches its edge.
 */
export function move_row_focus(event: KeyboardEvent) {
  const target = event.target;
  if (
    !KEYS.includes(event.key) ||
    !(target instanceof HTMLElement) ||
    !target.dataset.taskOpen ||
    !(event.currentTarget instanceof HTMLElement)
  ) {
    return;
  }
  const rows = [
    ...event.currentTarget.querySelectorAll<HTMLElement>("[data-task-open]"),
  ];
  const index = rows.indexOf(target);
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? rows.length - 1
        : index + (event.key === "ArrowDown" ? 1 : -1);
  if (index < 0 || next < 0 || next >= rows.length) {
    return;
  }
  event.preventDefault();
  rows[next].focus();
}
