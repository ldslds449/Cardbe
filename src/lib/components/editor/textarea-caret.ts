/**
 * Measures the top of the line at each source offset, relative to the
 * textarea's border box, with one hidden copy of the auto-sized input.
 */
export function caretTops(
  input: HTMLTextAreaElement,
  offsets: readonly number[],
): number[] {
  const mirror = input.cloneNode() as HTMLTextAreaElement;
  mirror.removeAttribute("id");
  mirror.setAttribute("aria-hidden", "true");
  mirror.tabIndex = -1;
  Object.assign(mirror.style, {
    position: "absolute",
    visibility: "hidden",
    width: `${input.offsetWidth}px`,
    height: "0px",
    minHeight: "0px",
  });
  input.after(mirror);
  const style = getComputedStyle(input);
  const lineHeight = parseFloat(style.lineHeight) || 20;
  const paddingBottom = parseFloat(style.paddingBottom) || 0;
  const tops = offsets.map((offset) => {
    mirror.value = input.value.slice(0, offset);
    return Math.max(0, mirror.scrollHeight - paddingBottom - lineHeight);
  });
  mirror.remove();
  return tops;
}

export function caretTop(input: HTMLTextAreaElement, offset: number): number {
  return caretTops(input, [offset])[0];
}
