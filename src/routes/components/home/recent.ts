// When each Focus, board, and note was last in view, so the home page can
// list recent ones first. This is a per-device convenience: losing it only
// restores the default order, so it lives in local storage, outside sync and
// backups.
const STORAGE_KEY = "cardbe.recent";
// Entries kept per kind; older ones fall back to the default order.
const MAX_ENTRIES = 200;

export type RecentKind = "focus" | "board" | "note";
export type RecentTimes = Record<RecentKind, Record<string, number>>;

export function read_recent(): RecentTimes {
  const empty: RecentTimes = { focus: {}, board: {}, note: {} };
  try {
    const stored = JSON.parse(
      window.localStorage.getItem(STORAGE_KEY) ?? "{}",
    ) as Partial<RecentTimes>;
    for (const kind of Object.keys(empty) as RecentKind[]) {
      const times = stored[kind];
      if (times && typeof times === "object") {
        empty[kind] = Object.fromEntries(
          Object.entries(times).filter(
            ([, time]) => typeof time === "number" && Number.isFinite(time),
          ),
        );
      }
    }
  } catch {
    // Unreadable storage keeps the default order.
  }
  return empty;
}

export function mark_recent(kind: RecentKind, id: string | number) {
  const recent = read_recent();
  recent[kind][String(id)] = Date.now();
  recent[kind] = Object.fromEntries(
    Object.entries(recent[kind])
      .sort(([, left], [, right]) => right - left)
      .slice(0, MAX_ENTRIES),
  );
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(recent));
  } catch {
    // The order is only a convenience.
  }
}

/** Most recent first; items never seen keep their order after the rest. */
export function by_recent<T>(
  items: readonly T[],
  time: (item: T) => number | undefined,
): T[] {
  return items
    .map((item, index) => ({ item, index, time: time(item) ?? -Infinity }))
    .sort((left, right) => right.time - left.time || left.index - right.index)
    .map(({ item }) => item);
}
