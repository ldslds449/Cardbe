/** A source scroll position and the target position that shows the same content. */
export type ScrollAnchor = [source: number, target: number];

/**
 * Builds increasing anchors from matching content positions, framed by the
 * start and end of both scroll ranges. Positions that would make the mapping
 * go backwards are dropped.
 */
export function scroll_anchors(
  pairs: readonly ScrollAnchor[],
  source_max: number,
  target_max: number,
): ScrollAnchor[] {
  const anchors: ScrollAnchor[] = [[0, 0]];
  for (const [source, target] of pairs) {
    const previous = anchors[anchors.length - 1];
    if (
      source > previous[0] &&
      target > previous[1] &&
      source < source_max &&
      target <= target_max
    ) {
      anchors.push([source, target]);
    }
  }
  anchors.push([
    Math.max(source_max, anchors[anchors.length - 1][0]),
    target_max,
  ]);
  return anchors;
}

/** Maps a source scroll position piecewise-linearly between anchors. */
export function map_scroll(
  anchors: readonly ScrollAnchor[],
  source: number,
): number {
  for (let index = 1; index < anchors.length; index += 1) {
    const [end_source, end_target] = anchors[index];
    if (source <= end_source) {
      const [start_source, start_target] = anchors[index - 1];
      const span = end_source - start_source;
      const progress = span > 0 ? (source - start_source) / span : 0;
      return start_target + Math.max(0, progress) * (end_target - start_target);
    }
  }
  return anchors[anchors.length - 1]?.[1] ?? 0;
}

/** Swaps source and target so the same anchors map the other direction. */
export function invert_anchors(
  anchors: readonly ScrollAnchor[],
): ScrollAnchor[] {
  return anchors.map(([source, target]) => [target, source]);
}
