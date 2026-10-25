<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import { formatDate, formatNumber } from "#lib/i18n/index.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import * as Select from "#lib/components/ui/select/index.js";
  import * as Table from "#lib/components/ui/table/index.js";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { untrack } from "svelte";
  import { translateCommandError } from "#lib/command-errors.js";
  import type { Archive } from "../../type/archive.svelte";
  import { done_column, type Column } from "../../type/column.svelte";
  import type { Task } from "../../type/task.svelte";
  import {
    lifecycle_ranges,
    task_lifecycle,
    type LifecycleRange,
  } from "./lifecycle";
  import {
    column_flow,
    completions,
    cumulative_flow,
    current_events,
    done_since,
  } from "./lifecycle-flow";
  import { load_task_events, type TaskEvent } from "./task-events";
  import CumulativeFlowChart from "./cumulative_flow_chart.svelte";

  let {
    board_id,
    archives,
    columns,
    now,
    onViewTask,
  }: {
    board_id: number | null;
    archives: Archive[];
    columns: Column[];
    now: Date;
    onViewTask: (task: Task) => void;
  } = $props();

  const AGING_LIMIT = 10;
  let range = $state("30");
  const range_value = $derived(
    (range === "all" ? "all" : Number(range)) as LifecycleRange,
  );

  // Refresh the log whenever tasks move, since each move may add events.
  const placement = $derived(
    columns
      .map((column) => `${column.id}:${column.tasks.map((t) => t.id)}`)
      .join("|") + `#${archives.map((archive) => archive.task.id)}`,
  );
  // The structured error is kept so its message follows language changes.
  let log = $state<
    { board_id: number; events: TaskEvent[] } | { error: unknown } | null
  >(null);
  $effect(() => {
    const id = board_id;
    if (id === null) {
      log = null;
      return;
    }
    let current = true;
    log = null;
    load_task_events(id)
      .then((events) => {
        if (current) {
          log = { board_id: id, events: since_reset(events) };
        }
      })
      .catch((error: unknown) => {
        if (current) {
          log = { error };
        }
      });
    return () => (current = false);
  });
  // The log only grows, so later moves fetch just the new events.
  $effect(() => {
    void placement;
    const target = untrack(() => (log && "events" in log ? log : null));
    if (!target) {
      return;
    }
    let current = true;
    const after = target.events.at(-1)?.id;
    load_task_events(target.board_id, { afterId: after })
      .then((events) => {
        if (current && events.length > 0 && log === target) {
          target.events = since_reset([...target.events, ...events]);
        }
      })
      .catch((error: unknown) => {
        if (current && log === target) {
          log = { error };
        }
      });
    return () => (current = false);
  });
  // Statistics only use events after the last board reset.
  function since_reset(events: TaskEvent[]) {
    const reset = events.findLastIndex((event) => event.kind === "board_reset");
    return reset > 0 ? events.slice(reset) : events;
  }
  const loaded = $derived(log && "events" in log ? log : null);
  // The board's done column, set from the column menu.
  const done_target = $derived(done_column(columns));
  const done = $derived(new Set(done_target ? [done_target.id] : []));
  const recorded = $derived(current_events(loaded?.events ?? []));
  const completed = $derived(
    completions(archives, columns, done, done_since(recorded.effective, done)),
  );
  const lifecycle = $derived(
    task_lifecycle(completed.completed, completed.active, range_value, now),
  );
  const flow = $derived(
    column_flow(recorded.effective, columns, done, range_value, now),
  );
  const flow_points = $derived(
    cumulative_flow(
      recorded.all,
      recorded.since,
      columns,
      archives.length,
      range_value,
      now,
    ),
  );

  const weekly_max = $derived(
    Math.max(1, ...lifecycle.weekly.flatMap((w) => [w.created, w.completed])),
  );
  const distribution_max = $derived(Math.max(1, ...lifecycle.distribution));
  const flow_change = $derived(
    lifecycle.weekly.reduce((sum, w) => sum + w.created - w.completed, 0),
  );
  const buckets = [
    m.statistics_bucket_under_1_day,
    m.statistics_bucket_1_3_days,
    m.statistics_bucket_3_7_days,
    m.statistics_bucket_1_2_weeks,
    m.statistics_bucket_2_weeks_1_month,
    m.statistics_bucket_over_1_month,
  ];

  function range_label(value: LifecycleRange | string) {
    return value === "all"
      ? m.statistics_all_time()
      : m.statistics_range_days({ count: Number(value) });
  }

  function label_name(value: string) {
    return value === "" ? m.statistics_no_label() : value;
  }

  function duration(ms: number) {
    const days = ms / (24 * 60 * 60 * 1000);
    return days < 1
      ? formatNumber(days * 24, {
          style: "unit",
          unit: "hour",
          unitDisplay: "long",
          maximumFractionDigits: days * 24 < 10 ? 1 : 0,
        })
      : formatNumber(days, {
          style: "unit",
          unit: "day",
          unitDisplay: "long",
          maximumFractionDigits: days < 10 ? 1 : 0,
        });
  }
</script>

<section class="flex flex-col gap-4" aria-labelledby="lifecycle-heading">
  <header class="flex flex-wrap items-end justify-between gap-3">
    <div class="space-y-1">
      <h2 id="lifecycle-heading" class="text-lg font-semibold">
        {m.statistics_lifecycle()}
      </h2>
      <p class="text-sm text-muted-foreground">
        {m.statistics_lifecycle_description()}
      </p>
      <p class="text-xs text-muted-foreground">
        {done_target
          ? m.statistics_done_column_named({ name: done_target.name })
          : m.statistics_done_column_none()}
      </p>
    </div>
    <div class="flex flex-wrap gap-2">
      <Select.Root type="single" bind:value={range}>
        <Select.Trigger class="w-36" aria-label={m.statistics_range()}
          >{range_label(range)}</Select.Trigger
        >
        <Select.Content>
          {#each lifecycle_ranges as item (item)}
            <Select.Item value={String(item)} label={range_label(item)}
              >{range_label(item)}</Select.Item
            >
          {/each}
        </Select.Content>
      </Select.Root>
    </div>
  </header>

  <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
    <Card.Root class="gap-3">
      <Card.Header>
        <Card.Description>{m.statistics_completed()}</Card.Description>
        <Card.Title class="text-3xl tabular-nums"
          >{formatNumber(lifecycle.completed)}</Card.Title
        >
      </Card.Header>
      <Card.Content class="text-xs text-muted-foreground"
        >{lifecycle.previous_completed === null
          ? range_label(range)
          : m.statistics_previous_period({
              count: lifecycle.previous_completed,
            })}</Card.Content
      >
    </Card.Root>
    <Card.Root class="gap-3">
      <Card.Header>
        <Card.Description>{m.statistics_lead_time_median()}</Card.Description>
        <Card.Title class="text-3xl tabular-nums"
          >{lifecycle.lead_time_median === null
            ? m.statistics_no_data()
            : duration(lifecycle.lead_time_median)}</Card.Title
        >
      </Card.Header>
      <Card.Content class="text-xs text-muted-foreground"
        >{lifecycle.lead_time_p85 === null
          ? m.statistics_lead_time_hint()
          : m.statistics_lead_time_p85({
              duration: duration(lifecycle.lead_time_p85),
            })}</Card.Content
      >
    </Card.Root>
    <Card.Root class="gap-3">
      <Card.Header>
        <Card.Description>{m.statistics_on_time_rate()}</Card.Description>
        <Card.Title class="text-3xl tabular-nums"
          >{lifecycle.due_completed === 0
            ? m.statistics_no_data()
            : formatNumber(lifecycle.on_time / lifecycle.due_completed, {
                style: "percent",
              })}</Card.Title
        >
      </Card.Header>
      <Card.Content class="text-xs text-muted-foreground"
        >{m.statistics_on_time_detail({
          on_time: lifecycle.on_time,
          count: lifecycle.due_completed,
        })}</Card.Content
      >
    </Card.Root>
    <Card.Root class="gap-3">
      <Card.Header>
        <Card.Description>{m.statistics_overdue_now()}</Card.Description>
        <Card.Title
          class={`text-3xl tabular-nums ${lifecycle.overdue ? "text-destructive" : ""}`}
          >{formatNumber(lifecycle.overdue)}</Card.Title
        >
      </Card.Header>
      <Card.Content class="text-xs text-muted-foreground"
        >{m.statistics_overdue_detail()}</Card.Content
      >
    </Card.Root>
  </div>
  {#if lifecycle.unknown_created > 0}
    <p class="text-xs text-muted-foreground">
      {m.statistics_unknown_created({ count: lifecycle.unknown_created })}
    </p>
  {/if}

  <div class="grid gap-4 lg:grid-cols-2">
    <Card.Root>
      <Card.Header>
        <Card.Title>{m.statistics_weekly_flow()}</Card.Title>
        <Card.Description
          >{flow_change > 0
            ? m.statistics_flow_growing({ count: flow_change })
            : flow_change < 0
              ? m.statistics_flow_shrinking({ count: -flow_change })
              : m.statistics_flow_balanced()}</Card.Description
        >
      </Card.Header>
      <Card.Content>
        <div class="flex h-32 items-end gap-px" role="list">
          {#each lifecycle.weekly as week (week.start.getTime())}
            {@const summary = m.statistics_week_summary({
              date: formatDate(week.start, { dateStyle: "medium" }),
              created: formatNumber(week.created),
              completed: formatNumber(week.completed),
            })}
            <div
              class="flex h-full min-w-0 flex-1 items-end justify-center gap-px"
              role="listitem"
              aria-label={summary}
              title={summary}
            >
              <span
                class="w-1/2 max-w-2 rounded-t-xs bg-muted-foreground/50"
                style:height={`${(week.created / weekly_max) * 100}%`}
              ></span>
              <span
                class="w-1/2 max-w-2 rounded-t-xs bg-success"
                style:height={`${(week.completed / weekly_max) * 100}%`}
              ></span>
            </div>
          {/each}
        </div>
        <div
          class="mt-2 flex justify-between text-[10px] text-muted-foreground"
          aria-hidden="true"
        >
          <span
            >{formatDate(lifecycle.weekly[0].start, {
              dateStyle: "medium",
            })}</span
          >
          <span
            >{formatDate(lifecycle.weekly[lifecycle.weekly.length - 1].start, {
              dateStyle: "medium",
            })}</span
          >
        </div>
        <div class="mt-3 flex gap-4 text-xs text-muted-foreground">
          <span class="flex items-center gap-1"
            ><span class="size-3 rounded-xs bg-muted-foreground/50"
            ></span>{m.statistics_created()}</span
          >
          <span class="flex items-center gap-1"
            ><span class="size-3 rounded-xs bg-success"
            ></span>{m.statistics_completed()}</span
          >
        </div>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title>{m.statistics_lead_time_distribution()}</Card.Title>
        <Card.Description>{m.statistics_lead_time_hint()}</Card.Description>
      </Card.Header>
      <Card.Content>
        {#if lifecycle.lead_time_count === 0}
          <p class="text-sm text-muted-foreground">
            {m.statistics_completed_empty()}
          </p>
        {:else}
          <dl
            class="grid grid-cols-[auto_1fr_auto] items-center gap-x-3 gap-y-2 text-sm"
          >
            {#each lifecycle.distribution as count, index}
              <dt class="text-muted-foreground">{buckets[index]()}</dt>
              <dd class="h-3 rounded-xs bg-muted" aria-hidden="true">
                <div
                  class="h-full rounded-xs bg-success"
                  style:width={`${(count / distribution_max) * 100}%`}
                ></div>
              </dd>
              <dd class="text-right tabular-nums">{formatNumber(count)}</dd>
            {/each}
          </dl>
        {/if}
      </Card.Content>
    </Card.Root>
  </div>

  <div class="grid gap-4">
    <Card.Root>
      <Card.Header>
        <Card.Title>{m.statistics_aging()}</Card.Title>
        <Card.Description>{m.statistics_aging_description()}</Card.Description>
      </Card.Header>
      <Card.Content>
        {#if lifecycle.aging.length === 0}
          <p class="text-sm text-muted-foreground">
            {m.statistics_aging_empty()}
          </p>
        {:else}
          <ul class="-mx-2 flex flex-col">
            {#each lifecycle.aging.slice(0, AGING_LIMIT) as item (item.task.id)}
              <li>
                <button
                  type="button"
                  class="flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent focus-visible:outline-2 focus-visible:outline-ring"
                  onclick={() => onViewTask(item.task)}
                >
                  <span class="min-w-0 flex-1">
                    <span class="block truncate font-medium"
                      >{item.task.title}</span
                    >
                    <span class="block truncate text-xs text-muted-foreground"
                      >{item.column_name}</span
                    >
                  </span>
                  <span class="shrink-0 text-right text-xs tabular-nums">
                    <span class="block text-muted-foreground"
                      >{m.statistics_aging_age({
                        duration: duration(item.age),
                      })}</span
                    >
                    {#if item.overdue !== null}
                      <span class="block text-destructive"
                        >{m.statistics_aging_overdue({
                          duration: duration(item.overdue),
                        })}</span
                      >
                    {/if}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </Card.Content>
    </Card.Root>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>{m.statistics_by_label()}</Card.Title>
      <Card.Description>{m.statistics_by_label_description()}</Card.Description>
    </Card.Header>
    <Card.Content>
      {#if lifecycle.by_label.length === 0}
        <p class="text-sm text-muted-foreground">
          {m.statistics_completed_empty()}
        </p>
      {:else}
        <Table.Root>
          <Table.Header>
            <Table.Row>
              <Table.Head>{m.statistics_label()}</Table.Head>
              <Table.Head class="text-right"
                >{m.statistics_completed()}</Table.Head
              >
              <Table.Head class="text-right"
                >{m.statistics_median_time()}</Table.Head
              >
              <Table.Head class="text-right"
                >{m.statistics_on_time_rate()}</Table.Head
              >
              <Table.Head class="text-right"
                >{m.statistics_overdue_now()}</Table.Head
              >
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each lifecycle.by_label as row (row.label)}
              <Table.Row>
                <Table.Cell class="max-w-48 truncate"
                  >{label_name(row.label)}</Table.Cell
                >
                <Table.Cell class="text-right tabular-nums"
                  >{formatNumber(row.completed)}</Table.Cell
                >
                <Table.Cell class="text-right tabular-nums"
                  >{row.median === null
                    ? m.statistics_no_data()
                    : duration(row.median)}</Table.Cell
                >
                <Table.Cell
                  class="text-right tabular-nums"
                  title={m.statistics_on_time_detail({
                    on_time: row.on_time,
                    count: row.due_completed,
                  })}
                  >{row.due_completed === 0
                    ? m.statistics_no_data()
                    : formatNumber(row.on_time / row.due_completed, {
                        style: "percent",
                      })}</Table.Cell
                >
                <Table.Cell
                  class={`text-right tabular-nums ${row.overdue ? "text-destructive" : ""}`}
                  >{formatNumber(row.overdue)}</Table.Cell
                >
              </Table.Row>
            {/each}
          </Table.Body>
        </Table.Root>
      {/if}
    </Card.Content>
  </Card.Root>
  <Card.Root>
    <Card.Header>
      <Card.Title>{m.statistics_column_flow()}</Card.Title>
      <Card.Description
        >{m.statistics_column_flow_description()}</Card.Description
      >
    </Card.Header>
    <Card.Content class="flex flex-col gap-3">
      {#if log && "error" in log}
        <p class="text-sm text-destructive" role="alert">
          {translateCommandError(log.error)}
        </p>
      {:else if flow_points.length < 2}
        <p class="text-sm text-muted-foreground">
          {m.statistics_flow_empty()}
        </p>
      {:else}
        <CumulativeFlowChart points={flow_points} {columns} {done} />
      {/if}
      {#if recorded.since}
        <p class="text-xs text-muted-foreground">
          {m.statistics_recorded_since({
            date: formatDate(recorded.since, { dateStyle: "medium" }),
          })}
        </p>
      {/if}
    </Card.Content>
  </Card.Root>

  <Card.Root>
    <Card.Header>
      <Card.Title>{m.statistics_column_times()}</Card.Title>
      <Card.Description
        >{m.statistics_column_times_description()}</Card.Description
      >
    </Card.Header>
    <Card.Content class="flex flex-col gap-4">
      <dl class="flex flex-wrap items-baseline gap-x-2 text-sm">
        <dt class="text-muted-foreground">{m.statistics_reopen_rate()}</dt>
        <dd class="font-medium tabular-nums">
          {flow.completed === 0
            ? m.statistics_no_data()
            : formatNumber(flow.reopened / flow.completed, {
                style: "percent",
              })}
        </dd>
        <dd class="text-xs text-muted-foreground">
          {m.statistics_reopen_detail({
            reopened: flow.reopened,
            count: flow.completed,
          })}
        </dd>
      </dl>
      <Table.Root>
        <Table.Header>
          <Table.Row>
            <Table.Head>{m.statistics_column()}</Table.Head>
            <Table.Head class="text-right">{m.statistics_current()}</Table.Head>
            <Table.Head class="text-right"
              >{m.statistics_median_time()}</Table.Head
            >
            <Table.Head class="text-right">{m.statistics_p85()}</Table.Head>
            <Table.Head class="text-right">{m.statistics_backflow()}</Table.Head
            >
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each flow.rows as row (row.column.id)}
            <Table.Row>
              <Table.Cell class="max-w-56">
                <span class="flex items-center gap-2">
                  <span class="truncate">{row.column.name}</span>
                  {#if flow.bottleneck === row.column.id}
                    <Badge variant="destructive"
                      >{m.statistics_bottleneck()}</Badge
                    >
                  {:else if done.has(row.column.id)}
                    <Badge variant="secondary"
                      >{m.statistics_done_column()}</Badge
                    >
                  {/if}
                </span>
              </Table.Cell>
              <Table.Cell class="text-right tabular-nums"
                >{formatNumber(row.current)}</Table.Cell
              >
              <Table.Cell class="text-right tabular-nums"
                >{row.median === null
                  ? m.statistics_no_data()
                  : duration(row.median)}</Table.Cell
              >
              <Table.Cell class="text-right tabular-nums"
                >{row.p85 === null
                  ? m.statistics_no_data()
                  : duration(row.p85)}</Table.Cell
              >
              <Table.Cell class="text-right tabular-nums"
                >{formatNumber(row.backflow)}</Table.Cell
              >
            </Table.Row>
          {/each}
        </Table.Body>
      </Table.Root>
    </Card.Content>
  </Card.Root>
</section>
