<script lang="ts">
  import { onMount } from "svelte";
  import ChevronLeftIcon from "@lucide/svelte/icons/chevron-left";
  import ChevronRightIcon from "@lucide/svelte/icons/chevron-right";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import type { Archive } from "../../type/archive.svelte";
  import { archive_activity } from "./activity";

  let {
    archives,
    loaded,
    loading,
    onRetry,
  }: {
    archives: Archive[];
    loaded: boolean;
    loading: boolean;
    onRetry: () => void;
  } = $props();
  let now = $state(new Date());
  let year = $state(new Date().getFullYear());
  let selected_date = $state<Date | null>(null);
  onMount(() => {
    const timer = setInterval(() => (now = new Date()), 60_000);
    return () => clearInterval(timer);
  });
  const activity = $derived(
    archive_activity(
      archives.map((archive) => archive.time),
      year,
      now,
    ),
  );
  const weeks = $derived(
    Array.from({ length: activity.days.length / 7 }, (_, index) =>
      activity.days.slice(index * 7, index * 7 + 7),
    ),
  );
  const levels = [
    "bg-muted",
    "bg-success/25",
    "bg-success/45",
    "bg-success/70",
    "bg-success",
  ];
  const date_formatter = new Intl.DateTimeFormat("en-US", {
    dateStyle: "full",
  });
  const month_formatter = new Intl.DateTimeFormat("en-US", { month: "short" });
  const selected_day = $derived(
    activity.days.find(
      (day) => day.date.getTime() === selected_date?.getTime(),
    ),
  );
</script>

<section
  class="mx-auto flex w-full max-w-5xl flex-col gap-6"
  aria-label="Statistics"
>
  <header>
    <h1 class="text-2xl font-semibold tracking-tight">Statistics</h1>
    <p class="mt-1 text-sm text-muted-foreground">
      Archive activity for the current board. Restored or deleted tasks are
      excluded.
    </p>
  </header>
  {#if loading}
    <div
      class="flex items-center gap-2 py-12 text-sm text-muted-foreground"
      role="status"
    >
      <Spinner /> Loading statistics...
    </div>
  {:else if !loaded}
    <div class="flex items-center gap-3 py-12" role="alert">
      <p class="text-sm text-muted-foreground">Couldn't load statistics.</p>
      <Button variant="outline" onclick={onRetry}>Retry</Button>
    </div>
  {:else}
    <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
      {#each [{ label: "Total archived", value: archives.length, detail: "All time" }, { label: "Archived this year", value: activity.total, detail: String(year) }, { label: "Days with archived tasks", value: activity.active_days, detail: `In ${year}` }, { label: "Longest streak", value: activity.longest_streak, detail: `Consecutive days in ${year}` }] as metric}
        <Card.Root class="gap-3">
          <Card.Header
            ><Card.Description>{metric.label}</Card.Description><Card.Title
              class="text-3xl tabular-nums">{metric.value}</Card.Title
            ></Card.Header
          >
          <Card.Content class="text-xs text-muted-foreground"
            >{metric.detail}</Card.Content
          >
        </Card.Root>
      {/each}
    </div>
    <Card.Root>
      <Card.Header class="flex flex-wrap items-center justify-between gap-4">
        <div class="space-y-1">
          <Card.Title>Activity</Card.Title><Card.Description
            >{activity.total} archived tasks in {year}</Card.Description
          >
        </div>
        <div class="flex items-center gap-2">
          <Button
            variant="outline"
            size="icon-sm"
            aria-label="Previous year"
            onclick={() => year--}><ChevronLeftIcon /></Button
          >
          <span
            class="min-w-12 text-center text-sm font-medium tabular-nums"
            aria-live="polite">{year}</span
          >
          <Button
            variant="outline"
            size="icon-sm"
            aria-label="Next year"
            disabled={year >= now.getFullYear()}
            onclick={() => year++}><ChevronRightIcon /></Button
          >
        </div>
      </Card.Header>
      <Card.Content>
        <div class="overflow-x-auto pb-2">
          <div class="flex min-w-max gap-2">
            <div
              class="grid grid-rows-8 gap-1 text-[10px] leading-3 text-muted-foreground"
              aria-hidden="true"
            >
              {#each ["", "", "Mon", "", "Wed", "", "Fri", ""] as label}<span
                  class="h-3">{label}</span
                >{/each}
            </div>
            <div class="flex gap-1">
              {#each weeks as week}
                <div class="flex w-3 flex-col gap-1">
                  <span
                    class="h-3 overflow-visible text-[10px] leading-3 text-muted-foreground"
                    >{week.some(
                      (day) => day.in_year && day.date.getDate() === 1,
                    )
                      ? month_formatter.format(
                          week.find(
                            (day) => day.in_year && day.date.getDate() === 1,
                          )!.date,
                        )
                      : ""}</span
                  >
                  {#each week as day}
                    {@const label = `${date_formatter.format(day.date)}: ${day.count} archived tasks`}
                    <button
                      type="button"
                      class={`size-3 shrink-0 rounded-xs focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${day.in_year ? levels[Math.min(day.count, 4)] : "invisible"} ${!day.available ? "opacity-40" : ""}`}
                      disabled={!day.available}
                      aria-label={label}
                      title={label}
                      onfocus={() => (selected_date = day.date)}
                      onclick={() => (selected_date = day.date)}
                    ></button>
                  {/each}
                </div>
              {/each}
            </div>
          </div>
        </div>
        <div
          class="mt-4 flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground"
        >
          <span aria-live="polite"
            >{selected_day
              ? `${date_formatter.format(selected_day.date)}: ${selected_day.count} archived tasks`
              : ""}</span
          >
          <div
            class="flex items-center gap-1"
            aria-label="Activity intensity: 0, 1, 2, 3, 4 or more tasks"
          >
            <span class="mr-1">Less</span>{#each levels as level}<span
                class={`size-3 rounded-xs ${level}`}
              ></span>{/each}<span class="ml-1">More</span>
          </div>
        </div>
      </Card.Content>
    </Card.Root>
  {/if}
</section>
