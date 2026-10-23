<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import CpuIcon from "@lucide/svelte/icons/cpu";
  import MemoryStickIcon from "@lucide/svelte/icons/memory-stick";

  import { Button } from "$lib/components/ui/button/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import {
    format_cpu,
    format_memory,
    HISTORY_LENGTH,
    resource_monitor,
    type UsagePoint,
  } from "./resource-usage.svelte";

  let open = $state(false);

  $effect(() => resource_monitor.subscribe());
  $effect(() => {
    if (open) {
      return resource_monitor.track_history();
    }
  });

  const usage = $derived(resource_monitor.usage);
  const cpu = $derived(format_cpu(usage?.total.cpu_percent));
  const memory = $derived(format_memory(usage?.total.memory_bytes));
  const label = $derived(m.resource_usage_label({ cpu, memory }));

  const WIDTH = 100;
  const HEIGHT = 32;
  /** An SVG path over the last minute; null values break the line. */
  function sparkline(values: (number | null)[], max: number) {
    const offset = HISTORY_LENGTH - values.length;
    let path = "";
    let drawing = false;
    values.forEach((value, index) => {
      if (value === null) {
        drawing = false;
        return;
      }
      const x = ((offset + index) / (HISTORY_LENGTH - 1)) * WIDTH;
      const y = HEIGHT - (Math.min(value, max) / max) * HEIGHT;
      path += `${drawing ? "L" : "M"}${x},${y}`;
      drawing = true;
    });
    return path;
  }
  const history = $derived(resource_monitor.history);
  const values = (pick: (point: UsagePoint) => number | null) =>
    history.map(pick);
  const cpu_values = $derived(values((point) => point.cpu_percent));
  const memory_values = $derived(values((point) => point.memory_bytes));
  const peak = (list: (number | null)[]) =>
    Math.max(0, ...list.filter((value) => value !== null));
  // Keep low CPU visible without exaggerating it.
  const cpu_scale = $derived(Math.max(10, peak(cpu_values)));
  const memory_scale = $derived(Math.max(1, peak(memory_values) * 1.2));
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        variant="ghost"
        size="sm"
        class="h-8 shrink-0 gap-1 px-1.5 text-xs font-normal tabular-nums text-muted-foreground"
        aria-label={label}
      >
        <CpuIcon class="size-3.5" />
        <span class="hidden min-w-[5ch] text-end lg:inline">{cpu}</span>
        <MemoryStickIcon class="ms-1 hidden size-3.5 lg:block" />
        <span class="hidden min-w-[7ch] text-end lg:inline">{memory}</span>
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content align="end" class="w-72 p-3">
    <div class="flex flex-col gap-2 text-sm">
      <h2 class="font-medium">{m.resource_usage_title()}</h2>

      {#if resource_monitor.status === "unavailable"}
        <p class="text-xs text-destructive" role="status">
          {m.resource_usage_unavailable()}
        </p>
      {/if}

      <div class="grid grid-cols-2 gap-2">
        {#each [{ name: m.resource_usage_cpu_total(), value: cpu, values: cpu_values, scale: cpu_scale, peak: format_cpu(history.length ? peak(cpu_values) : null) }, { name: m.resource_usage_memory_total(), value: memory, values: memory_values, scale: memory_scale, peak: format_memory(history.length ? peak(memory_values) : null) }] as item (item.name)}
          <div class="flex flex-col gap-0.5 rounded-md border px-2 py-1.5">
            <span class="text-xs text-muted-foreground">{item.name}</span>
            <span class="text-base font-semibold tabular-nums"
              >{item.value}</span
            >
            <svg
              class="h-6 w-full text-primary"
              viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
              preserveAspectRatio="none"
              role="img"
              aria-label={m.resource_usage_history({
                name: item.name,
                peak: item.peak,
              })}
            >
              <line
                x1="0"
                x2={WIDTH}
                y1={HEIGHT}
                y2={HEIGHT}
                class="stroke-border"
                vector-effect="non-scaling-stroke"
              />
              <path
                d={sparkline(item.values, item.scale)}
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linejoin="round"
                vector-effect="non-scaling-stroke"
              />
            </svg>
          </div>
        {/each}
      </div>

      <table class="w-full text-xs tabular-nums">
        <thead class="text-muted-foreground">
          <tr>
            <th class="py-0.5 text-start font-normal"
              >{m.resource_usage_process_group()}</th
            >
            <th class="py-0.5 text-end font-normal">{m.resource_usage_cpu()}</th
            >
            <th class="py-0.5 text-end font-normal"
              >{m.resource_usage_memory()}</th
            >
          </tr>
        </thead>
        <tbody>
          {#each [{ name: m.resource_usage_main_process(), group: usage?.main }, { name: m.resource_usage_child_processes(), group: usage?.children }] as row (row.name)}
            <tr class="border-t">
              <th class="py-0.5 text-start font-normal">{row.name}</th>
              <td class="py-0.5 text-end"
                >{format_cpu(row.group?.cpu_percent)}</td
              >
              <td class="py-0.5 text-end"
                >{format_memory(row.group?.memory_bytes)}</td
              >
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="text-xs text-muted-foreground">
        {usage
          ? m.resource_usage_process_count({
              count: usage.total.process_count,
            })
          : m.resource_usage_process_count_unavailable()}
      </p>
    </div>
  </Popover.Content>
</Popover.Root>
