<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import { formatDate, formatNumber } from "#lib/i18n/index.js";
  import type { Column } from "../../type/column.svelte";
  import { flow_groups, type FlowPoint } from "./lifecycle-flow";

  let {
    points,
    columns,
    done,
  }: { points: FlowPoint[]; columns: Column[]; done: Set<string> } = $props();

  const WIDTH = 100;
  const HEIGHT = 100;

  // Bands from the top: board columns in order, then the archive.
  const bands = $derived.by(() => {
    const groups = flow_groups(columns, done);
    // One theme hue, deepening as tasks move along the board.
    const shade = (index: number) =>
      `color-mix(in oklab, var(--primary) ${Math.round(20 + (55 * (index + 1)) / groups.length)}%, var(--card))`;
    const list = groups.map(({ column, indexes }, index) => ({
      key: column?.id ?? "other",
      name: column?.name ?? m.statistics_other_columns(),
      fill: shade(index),
      value: (point: FlowPoint) =>
        indexes.reduce((sum, i) => sum + point.columns[i], 0),
    }));
    list.push({
      key: "archived",
      name: m.statistics_archived(),
      // Matches completed tasks in the weekly chart.
      fill: "var(--success)",
      // Only tasks archived in the range, so older ones do not dwarf columns.
      value: (point) => Math.max(0, point.archived - points[0].archived),
    });
    return list;
  });
  const max = $derived(
    Math.max(
      1,
      ...points.map((point) =>
        bands.reduce((sum, band) => sum + band.value(point), 0),
      ),
    ),
  );
  const x = (index: number) => (index / (points.length - 1)) * WIDTH;
  const y = (value: number) => HEIGHT - (value / max) * HEIGHT;
  const paths = $derived.by(() => {
    // Stack from the bottom so the archive rests on the baseline.
    const below = points.map(() => 0);
    return bands.toReversed().map((band) => {
      const lower = [...below];
      points.forEach((point, index) => (below[index] += band.value(point)));
      const top = below.map((value, index) => `${x(index)},${y(value)}`);
      const bottom = lower
        .map((value, index) => `${x(index)},${y(value)}`)
        .toReversed();
      return { ...band, d: `M${top.join("L")}L${bottom.join("L")}Z` };
    });
  });

  let hover = $state<number | null>(null);
  function step(event: KeyboardEvent) {
    const last = points.length - 1;
    const next = {
      ArrowLeft: (hover ?? last) - 1,
      ArrowRight: (hover ?? last) + 1,
      Home: 0,
      End: last,
    }[event.key];
    if (next !== undefined) {
      event.preventDefault();
      hover = Math.min(last, Math.max(0, next));
    }
  }
  function track(event: PointerEvent) {
    const rect = (event.currentTarget as Element).getBoundingClientRect();
    const ratio = (event.clientX - rect.left) / rect.width;
    hover = Math.min(
      points.length - 1,
      Math.max(0, Math.round(ratio * (points.length - 1))),
    );
  }
  const hint = $props.id();
  const date = (point: FlowPoint) =>
    formatDate(point.time, { dateStyle: "medium" });
</script>

<div class="flex flex-col gap-3">
  <div class="relative">
    <!-- Arrow keys pick a day, like moving the pointer across the chart. -->
    <svg
      class="h-48 w-full touch-none rounded-sm outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      preserveAspectRatio="none"
      role="slider"
      tabindex="0"
      aria-label={m.statistics_column_flow_description()}
      aria-describedby={hint}
      aria-valuemin={0}
      aria-valuemax={points.length - 1}
      aria-valuenow={hover ?? points.length - 1}
      aria-valuetext={date(points[hover ?? points.length - 1])}
      onpointermove={track}
      onpointerleave={() => (hover = null)}
      onfocus={() => (hover ??= points.length - 1)}
      onblur={() => (hover = null)}
      onkeydown={step}
    >
      {#each paths as band (band.key)}
        <path d={band.d} style:fill={band.fill} />
      {/each}
      {#if hover !== null}
        <line
          x1={x(hover)}
          x2={x(hover)}
          y1="0"
          y2={HEIGHT}
          class="stroke-foreground/50"
          stroke-width="1"
          stroke-dasharray="3 3"
          vector-effect="non-scaling-stroke"
        />
      {/if}
    </svg>
    {#if hover !== null}
      {@const point = points[hover]}
      <div
        aria-hidden="true"
        class="pointer-events-none absolute top-2 z-10 min-w-36 rounded-md border bg-popover px-3 py-2 text-xs text-popover-foreground shadow-md"
        style:left={`${(hover / (points.length - 1)) * 100}%`}
        style:transform={`translateX(${hover / (points.length - 1) > 0.5 ? "calc(-100% - 8px)" : "8px"})`}
      >
        <p class="mb-1 font-medium">{date(point)}</p>
        <dl
          class="grid grid-cols-[auto_1fr_auto] items-center gap-x-2 gap-y-0.5"
        >
          {#each bands as band (band.key)}
            <svg class="size-2.5" viewBox="0 0 1 1" aria-hidden="true"
              ><rect
                width="1"
                height="1"
                rx="0.2"
                style:fill={band.fill}
              /></svg
            >
            <dt class="truncate text-muted-foreground">{band.name}</dt>
            <dd class="text-right tabular-nums">
              {formatNumber(band.value(point))}
            </dd>
          {/each}
        </dl>
      </div>
    {/if}
  </div>
  <p id={hint} class="sr-only">{m.statistics_flow_keyboard_hint()}</p>
  <!-- The tooltip comes and goes, so counts are announced from here. -->
  <ul class="sr-only" aria-live="polite">
    {#if hover !== null}
      {#each bands as band (band.key)}
        <li>{band.name} {formatNumber(band.value(points[hover]))}</li>
      {/each}
    {/if}
  </ul>
  <div
    class="flex justify-between text-[10px] text-muted-foreground"
    aria-hidden="true"
  >
    <span>{date(points[0])}</span>
    <span>{date(points[points.length - 1])}</span>
  </div>
  <ul class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
    {#each bands as band (band.key)}
      <li class="flex min-w-0 items-center gap-1">
        <svg class="size-3 shrink-0" viewBox="0 0 1 1" aria-hidden="true"
          ><rect width="1" height="1" rx="0.2" style:fill={band.fill} /></svg
        >
        <span class="max-w-40 truncate">{band.name}</span>
      </li>
    {/each}
  </ul>
</div>
