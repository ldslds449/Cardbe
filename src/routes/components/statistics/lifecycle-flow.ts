import type { Archive } from "../../type/archive.svelte";
import type { Column } from "../../type/column.svelte";
import { median, percentile, type LifecycleRange } from "./lifecycle";
import type { TaskEvent } from "./task-events";

const DAY = 24 * 60 * 60 * 1000;
// A column needs this many finished stays before it can be the bottleneck.
export const BOTTLENECK_MIN_SAMPLES = 3;
const MAX_FLOW_POINTS = 120;
// Later columns fold into one band so shades stay distinguishable.
const NAMED_COLUMNS = 4;

/**
 * Events after the last board reset. `all` keeps undo operations, which
 * really changed the board; `effective` drops them with what they reverted.
 */
export function current_events(events: TaskEvent[]) {
  const reset = events.findLastIndex((event) => event.kind === "board_reset");
  const all = events.slice(reset + 1);
  const reverted = new Set(
    all.flatMap((event) =>
      event.origin === "undo" && event.reverts_op_id !== null
        ? [event.reverts_op_id]
        : [],
    ),
  );
  return {
    /** When recording of the current board state began. */
    since: reset >= 0 ? events[reset].at : (all[0]?.at ?? null),
    all,
    effective: all.filter(
      (event) => event.origin !== "undo" && !reverted.has(event.op_id),
    ),
  };
}

/** Whether the task counts as done after leaving or reaching a place. */
function done_state(
  event: TaskEvent,
  column: string | null,
  done: Set<string>,
): boolean | null {
  if (column !== null) {
    return done.has(column);
  }
  // Without a column the task is in the archive, or gone or new.
  return event.kind === "archived" || event.kind === "restored" ? true : null;
}

/**
 * Tasks done by sitting in a done column, with when they last arrived there
 * from unfinished work. Moving out of done columns reopens a task.
 */
export function done_since(effective: TaskEvent[], done: Set<string>) {
  const since = new Map<string, Date>();
  for (const event of effective) {
    const before = done_state(event, event.from_column_id, done);
    if (event.kind === "archived") {
      // The archive time completes tasks that were not already done.
      if (!before) {
        since.delete(event.task_id);
      }
    } else if (event.to_column_id === null || !done.has(event.to_column_id)) {
      since.delete(event.task_id);
    } else if (
      !before ||
      (event.kind === "restored" && !since.has(event.task_id))
    ) {
      // A restored task loses its archive time, so it is done from now.
      since.set(event.task_id, event.at);
    }
  }
  return since;
}

/**
 * Completions for the lifecycle overview: archived tasks, and tasks in done
 * columns with a known arrival. `active` leaves done columns out.
 */
export function completions(
  archives: Archive[],
  columns: Column[],
  done: Set<string>,
  since: Map<string, Date>,
) {
  const completed: Archive[] = archives.map((archive) => ({
    task: archive.task,
    time: since.get(archive.task.id) ?? archive.time,
  }));
  for (const column of columns) {
    if (done.has(column.id)) {
      for (const task of column.tasks) {
        const time = since.get(task.id);
        if (time) {
          completed.push({ task, time });
        }
      }
    }
  }
  return {
    completed,
    active: columns.filter((column) => !done.has(column.id)),
  };
}

export interface ColumnFlow {
  column: Column;
  current: number;
  /** Finished stays that ended in the range. */
  samples: number;
  median: number | null;
  p85: number | null;
  /** Moves from this column to an earlier one in the range. */
  backflow: number;
}

/** How tasks moved through the board's columns during the range. */
export function column_flow(
  effective: TaskEvent[],
  columns: Column[],
  done: Set<string>,
  range: LifecycleRange,
  now: Date,
) {
  const end = now.getTime();
  const start = range === "all" ? -Infinity : end - range * DAY;
  const in_range = (time: Date) =>
    time.getTime() > start && time.getTime() <= end;
  const index = new Map(columns.map((column, i) => [column.id, i]));
  const stays = new Map(columns.map((column) => [column.id, [] as number[]]));
  const backflow = new Map(columns.map((column) => [column.id, 0]));
  const entered = new Map<string, { column: string; at: Date }>();
  let completed = 0;
  let reopened = 0;
  for (const event of effective) {
    const from = event.from_column_id;
    const stay = entered.get(event.task_id);
    if (from !== null && stay?.column === from && in_range(event.at)) {
      stays.get(from)?.push(event.at.getTime() - stay.at.getTime());
    }
    entered.delete(event.task_id);
    if (event.to_column_id !== null) {
      entered.set(event.task_id, { column: event.to_column_id, at: event.at });
    }
    if (!in_range(event.at)) {
      continue;
    }
    if (
      event.kind === "moved" &&
      from !== null &&
      event.to_column_id !== null &&
      (index.get(from) ?? -1) > (index.get(event.to_column_id) ?? Infinity)
    ) {
      backflow.set(from, (backflow.get(from) ?? 0) + 1);
    }
    const before = done_state(event, from, done);
    const after = done_state(event, event.to_column_id, done);
    if (!before && after) {
      completed++;
    } else if (before && after === false) {
      reopened++;
    }
  }
  const rows: ColumnFlow[] = columns.map((column) => {
    const times = stays.get(column.id)!.sort((a, b) => a - b);
    return {
      column,
      current: column.tasks.length,
      samples: times.length,
      median: median(times),
      p85: percentile(times, 0.85),
      backflow: backflow.get(column.id)!,
    };
  });
  let bottleneck: string | null = null;
  let longest = -1;
  for (const row of rows) {
    if (
      !done.has(row.column.id) &&
      row.samples >= BOTTLENECK_MIN_SAMPLES &&
      row.median! > longest
    ) {
      bottleneck = row.column.id;
      longest = row.median!;
    }
  }
  return { rows, bottleneck, completed, reopened };
}

export interface FlowPoint {
  time: Date;
  archived: number;
  /** Task counts in the order of the board's columns. */
  columns: number[];
}

/**
 * Daily task counts per column and in the archive, rebuilt backwards from
 * the current board through every recorded change, undos included.
 */
export function cumulative_flow(
  all: TaskEvent[],
  since: Date | null,
  columns: Column[],
  archived: number,
  range: LifecycleRange,
  now: Date,
): FlowPoint[] {
  if (since === null) {
    return [];
  }
  const end = now.getTime();
  const first = Math.max(
    since.getTime(),
    range === "all" ? -Infinity : end - range * DAY,
  );
  const step = Math.max(
    DAY,
    Math.ceil((end - first) / DAY / MAX_FLOW_POINTS) * DAY,
  );
  const times: number[] = [];
  for (let time = end; time > first; time -= step) {
    times.push(time);
  }
  times.push(first);

  const index = new Map(columns.map((column, i) => [column.id, i]));
  const counts = columns.map((column) => column.tasks.length);
  const points: FlowPoint[] = [];
  let next = all.length - 1;
  for (const time of times) {
    for (; next >= 0 && all[next].at.getTime() > time; next--) {
      const event = all[next];
      const to =
        event.to_column_id === null ? undefined : index.get(event.to_column_id);
      const from =
        event.from_column_id === null
          ? undefined
          : index.get(event.from_column_id);
      if (to !== undefined) {
        counts[to]--;
      } else if (event.kind === "archived") {
        archived--;
      }
      if (from !== undefined) {
        counts[from]++;
      } else if (
        event.from_column_id === null &&
        (event.kind === "restored" || event.kind === "deleted")
      ) {
        archived++;
      }
    }
    points.push({
      time: new Date(time),
      archived: Math.max(0, archived),
      columns: counts.map((count) => Math.max(0, count)),
    });
  }
  return points.reverse();
}

/**
 * Chart bands for the board's columns in order. On wide boards, columns past
 * the first few fold into one band (`column: null`) where the first of them
 * would be; done columns were chosen to be watched, so they never fold.
 */
export function flow_groups(columns: Column[], done: Set<string>) {
  let open = Math.max(
    0,
    NAMED_COLUMNS - columns.filter((column) => done.has(column.id)).length,
  );
  const groups: { column: Column | null; indexes: number[] }[] = [];
  let folded: number[] | null = null;
  columns.forEach((column, index) => {
    if (
      columns.length <= NAMED_COLUMNS + 1 ||
      done.has(column.id) ||
      open-- > 0
    ) {
      groups.push({ column, indexes: [index] });
    } else if (folded) {
      folded.push(index);
    } else {
      folded = [index];
      groups.push({ column: null, indexes: folded });
    }
  });
  return groups;
}
