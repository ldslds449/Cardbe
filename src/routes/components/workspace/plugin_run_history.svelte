<script lang="ts">
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Spinner } from "#lib/components/ui/spinner/index.js";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CircleSlashIcon from "@lucide/svelte/icons/circle-slash";
  import CircleDashedIcon from "@lucide/svelte/icons/circle-dashed";
  import ShieldAlertIcon from "@lucide/svelte/icons/shield-alert";
  import { m } from "#lib/paraglide/messages.js";
  import { formatDateTime, formatNumber } from "#lib/i18n/index.js";
  import type { CommandError } from "#lib/command-errors.js";
  import {
    pluginErrorMessage,
    pluginLogBody,
    pluginLogLevel,
    pluginRunStatus,
    type PluginRun,
    type PluginRunLog,
  } from "#lib/plugins.js";

  let {
    open = $bindable(false),
    name,
    runs,
    pending,
    error,
  }: {
    open?: boolean;
    name: string;
    runs: PluginRun[];
    pending: boolean;
    error: CommandError | null;
  } = $props();

  const levels = ["error", "warn", "info", "debug"] as const;
  let selectedId = $state<string | null>(null);
  let hidden = $state<Partial<Record<PluginRunLog["level"], boolean>>>({});
  // Runs arrive newest first; keep the selection across polls and fall back to the latest run.
  const selected = $derived(
    runs.find((run) => run.id === selectedId) ?? runs[0] ?? null,
  );
  const levelCounts = $derived(
    Object.fromEntries(
      levels.map((level) => [
        level,
        selected?.logs.filter((log) => log.level === level).length ?? 0,
      ]),
    ) as Record<PluginRunLog["level"], number>,
  );
  const visibleLogs = $derived(
    selected?.logs.filter((log) => !hidden[log.level]) ?? [],
  );
  $effect(() => {
    if (!open) {
      selectedId = null;
      hidden = {};
    }
  });

  function trigger(run: PluginRun) {
    return run.trigger === "manual"
      ? m.plugin_manual_trigger()
      : m.plugin_schedule_trigger();
  }
  function duration(run: PluginRun) {
    return run.finished_at
      ? m.plugin_duration({
          seconds: formatNumber((run.finished_at - run.started_at) / 1000, {
            maximumFractionDigits: 1,
          }),
        })
      : null;
  }
  const statusTone: Record<PluginRun["status"], string> = {
    running: "text-muted-foreground",
    success: "text-success",
    failed: "text-destructive",
    cancelled: "text-muted-foreground",
    interrupted: "text-warning",
    awaiting_permission: "text-warning",
  };
  const levelTone: Record<PluginRunLog["level"], string> = {
    error: "text-destructive",
    warn: "text-warning",
    info: "text-foreground",
    debug: "text-muted-foreground",
  };
</script>

{#snippet statusIcon(value: PluginRun["status"])}
  {#if value === "running"}<Spinner aria-hidden="true" />
  {:else if value === "success"}<CircleCheckIcon aria-hidden="true" />
  {:else if value === "failed"}<CircleAlertIcon aria-hidden="true" />
  {:else if value === "cancelled"}<CircleSlashIcon aria-hidden="true" />
  {:else if value === "awaiting_permission"}<ShieldAlertIcon
      aria-hidden="true"
    />
  {:else}<CircleDashedIcon aria-hidden="true" />{/if}
{/snippet}

<Dialog.Root bind:open>
  <Dialog.Content
    class="flex h-[85dvh] max-h-200 flex-col overflow-hidden sm:max-w-4xl"
  >
    <Dialog.Header>
      <Dialog.Title>{m.plugin_logs()}</Dialog.Title>
      <Dialog.Description>{name}</Dialog.Description>
    </Dialog.Header>
    {#if error}<p role="alert" class="text-sm text-destructive">
        {pluginErrorMessage(error)}
      </p>{/if}
    {#if !runs.length}
      <p role="status" class="text-sm text-muted-foreground">
        {pending ? m.ui_loading() : m.plugin_no_runs()}
      </p>
    {:else}
      <div
        class="grid min-h-0 flex-1 grid-rows-[minmax(0,11rem)_minmax(0,1fr)] gap-4 md:grid-cols-[15rem_minmax(0,1fr)] md:grid-rows-1"
      >
        <ScrollArea class="min-h-0 rounded-md border">
          <ul class="space-y-0.5 p-1" aria-label={m.plugin_run_list()}>
            {#each runs as run (run.id)}<li>
                <Button
                  variant="ghost"
                  aria-current={run.id === selected?.id ? "true" : undefined}
                  class={[
                    "h-auto w-full justify-start gap-2 px-2 py-1.5 text-start font-normal",
                    run.id === selected?.id &&
                      "bg-accent text-accent-foreground",
                  ]}
                  onclick={() => (selectedId = run.id)}
                >
                  <span class={["mt-0.5 self-start", statusTone[run.status]]}
                    >{@render statusIcon(run.status)}</span
                  >
                  <span class="min-w-0 flex-1">
                    <span class="block truncate text-sm"
                      >{formatDateTime(run.started_at)}</span
                    >
                    <span class="block truncate text-xs text-muted-foreground"
                      >{pluginRunStatus(run.status)} · {trigger(run)}</span
                    >
                  </span>
                </Button>
              </li>{/each}
          </ul>
        </ScrollArea>
        {#if selected}
          <section
            class="flex min-h-0 flex-col gap-3"
            aria-label={formatDateTime(selected.started_at)}
          >
            <div class="space-y-1.5">
              <div class="flex flex-wrap items-center gap-2">
                <Badge
                  variant="outline"
                  class={["gap-1.5", statusTone[selected.status]]}
                  >{@render statusIcon(selected.status)}{pluginRunStatus(
                    selected.status,
                  )}</Badge
                >
                <span class="text-sm font-medium"
                  >{formatDateTime(selected.started_at)}</span
                >
              </div>
              <p class="text-sm text-muted-foreground">
                {[
                  trigger(selected),
                  duration(selected),
                  m.plugin_run_counts({
                    created: formatNumber(selected.created),
                    updated: formatNumber(selected.updated),
                  }),
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </p>
              <p
                class="break-all font-mono text-xs text-muted-foreground select-text"
              >
                {m.plugin_run_id({ id: selected.id })}
              </p>
              {#if selected.error}<p
                  class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
                >
                  {pluginErrorMessage(selected.error)}
                </p>{/if}
            </div>
            {#if selected.logs.length}
              <div
                class="flex flex-wrap gap-1.5"
                role="group"
                aria-label={m.plugin_log_filter()}
              >
                {#each levels.filter((level) => levelCounts[level]) as level (level)}<Button
                    size="sm"
                    variant={hidden[level] ? "ghost" : "secondary"}
                    aria-pressed={!hidden[level]}
                    class="h-7 px-2 text-xs"
                    onclick={() => (hidden[level] = !hidden[level])}
                    >{pluginLogLevel(level)}
                    <span class="text-muted-foreground"
                      >{formatNumber(levelCounts[level])}</span
                    ></Button
                  >{/each}
              </div>
            {/if}
            <ScrollArea
              type="always"
              class="min-h-0 flex-1 rounded-md border bg-muted/40"
            >
              {#if !selected.logs.length}
                <p class="p-3 text-sm text-muted-foreground">
                  {m.plugin_no_logs()}
                </p>
              {:else if !visibleLogs.length}
                <p class="p-3 text-sm text-muted-foreground">
                  {m.plugin_logs_filtered_empty()}
                </p>
              {:else}
                <ol class="py-2 pe-3 font-mono text-xs select-text">
                  {#each visibleLogs as log, i (i)}<li
                      class="grid grid-cols-[4.5rem_minmax(0,1fr)] gap-2 px-3 py-0.5 hover:bg-muted"
                    >
                      <span class={["font-medium", levelTone[log.level]]}
                        >{pluginLogLevel(log.level)}</span
                      >
                      <span
                        class={[
                          "whitespace-pre-wrap break-words",
                          log.level === "error" && "text-destructive",
                          log.level === "debug" && "text-muted-foreground",
                        ]}>{pluginLogBody(log)}</span
                      >
                    </li>{/each}
                </ol>
              {/if}
            </ScrollArea>
          </section>
        {/if}
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
