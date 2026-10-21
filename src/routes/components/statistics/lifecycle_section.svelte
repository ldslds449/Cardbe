<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import { formatDate, formatNumber } from "$lib/i18n";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as Table from "$lib/components/ui/table/index.js";
  import type { Archive } from "../../type/archive.svelte";
  import type { Column } from "../../type/column.svelte";
  import type { Task } from "../../type/task.svelte";
  import {
    lifecycle_labels,
    lifecycle_ranges,
    task_lifecycle,
    type LifecycleRange,
  } from "./lifecycle";

  let {
    archives,
    columns,
    now,
    onViewTask,
  }: {
    archives: Archive[];
    columns: Column[];
    now: Date;
    onViewTask: (task: Task) => void;
  } = $props();

  const AGING_LIMIT = 10;
  // Select values are strings; these sentinels cannot be label text.
  const ALL_LABELS = "\u0000all";
  const NO_LABEL = "\u0000none";
  let range = $state("30");
  let label = $state(ALL_LABELS);
  const labels = $derived(lifecycle_labels(archives, columns));
  const lifecycle = $derived(
    task_lifecycle(
      archives,
      columns,
      {
        range: (range === "all" ? "all" : Number(range)) as LifecycleRange,
        label: label === ALL_LABELS ? null : label === NO_LABEL ? "" : label,
      },
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
    return value === ALL_LABELS
      ? m.statistics_all_labels()
      : value === NO_LABEL || value === ""
        ? m.statistics_no_label()
        : value;
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
    </div>
    <div class="flex flex-wrap items-center gap-2">
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
      <Select.Root type="single" bind:value={label}>
        <Select.Trigger class="w-40" aria-label={m.statistics_label_filter()}
          ><span class="truncate">{label_name(label)}</span></Select.Trigger
        >
        <Select.Content>
          {#each [ALL_LABELS, NO_LABEL, ...labels] as item (item)}
            <Select.Item value={item} label={label_name(item)}
              >{label_name(item)}</Select.Item
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

  <div class="grid gap-4 lg:grid-cols-2">
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

    <Card.Root>
      <Card.Header>
        <Card.Title>{m.statistics_by_label()}</Card.Title>
        <Card.Description>{range_label(range)}</Card.Description>
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
                <Table.Head>{m.statistics_label_filter()}</Table.Head>
                <Table.Head class="text-right"
                  >{m.statistics_completed()}</Table.Head
                >
                <Table.Head class="text-right"
                  >{m.statistics_lead_time_median()}</Table.Head
                >
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each lifecycle.by_label as row (row.label)}
                <Table.Row>
                  <Table.Cell class="max-w-40 truncate"
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
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        {/if}
      </Card.Content>
    </Card.Root>
  </div>
</section>
