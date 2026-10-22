// Bits UI keeps a context menu open when another context-menu trigger is
// right-clicked, so each card's menu would stay open beside the next one.
// Opening a menu through `claim_context_menu` closes the previous one.
let close_current: (() => void) | undefined;

export function claim_context_menu(close: () => void) {
  if (close_current !== close) {
    close_current?.();
  }
  close_current = close;
}

export function release_context_menu(close: () => void) {
  if (close_current === close) {
    close_current = undefined;
  }
}
