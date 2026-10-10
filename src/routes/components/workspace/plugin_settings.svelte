<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openFile } from "@tauri-apps/plugin-dialog";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { m } from "$lib/paraglide/messages.js";
  import { formatDateTime, formatNumber } from "$lib/i18n";
  import { parseCommandError, type CommandError } from "$lib/command-errors";
  import {
    invalidPluginSettings,
    pluginErrorMessage,
    pluginLogMessage,
    pluginDefaults,
    pluginText,
    type PluginState,
    type PluginPackage,
    type PluginInstance,
    type PluginRun,
  } from "$lib/plugins";
  import { board } from "../../board.svelte";

  let { open = $bindable(false) }: { open: boolean } = $props();
  let pluginState = $state<PluginState>({
    packages: [],
    instances: [],
    safe_mode: false,
  });
  let error = $state<CommandError | null>(null);
  let pending = $state(false);
  let loaded = $state(false);
  let editing = $state<PluginPackage | null>(null);
  let instanceId = $state<string | undefined>();
  let name = $state("");
  let boardId = $state("");
  let config = $state<Record<string, unknown>>({});
  let secrets = $state<Record<string, string>>({});
  let savedSecrets = $state<string[]>([]);
  let domains = $state<string[]>([]);
  let enabled = $state(false);
  let interval = $state<number | undefined>(0);
  let consent = $state(false);
  let invalid = $state<string[]>([]);
  let columns = $state<{ id: number; name: string }[]>([]);
  let columnRequest = 0;
  let session = 0;
  let refreshRequest: Promise<void> | null = null;
  let historyId = $state<string | null>(null);
  let runs = $state<PluginRun[]>([]);
  let removal = $state<{
    kind: "package" | "instance";
    id: string;
  } | null>(null);

  async function refresh() {
    if (refreshRequest) {
      return refreshRequest;
    }
    const generation = session;
    const requestedHistory = historyId;
    refreshRequest = (async () => {
      const next = await invoke<PluginState>("get_plugin_state");
      const history = requestedHistory
        ? await invoke<PluginRun[]>("get_plugin_runs", {
            instanceId: requestedHistory,
          })
        : [];
      if (generation !== session) {
        return;
      }
      pluginState = next;
      loaded = true;
      if (historyId === requestedHistory) {
        runs = history;
      }
    })();
    try {
      await refreshRequest;
    } finally {
      refreshRequest = null;
    }
  }
  async function action(work: () => Promise<unknown>) {
    if (pending) {
      return;
    }
    const generation = session;
    pending = true;
    error = null;
    try {
      await work();
      await refresh();
    } catch (cause) {
      if (generation === session) {
        error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
      }
    } finally {
      pending = false;
    }
  }
  $effect(() => {
    session++;
    if (!open) {
      secrets = {};
      columnRequest++;
      return;
    }
    untrack(() => void action(refresh));
    const generation = session;
    const timer = setInterval(() => {
      if (!pending) {
        void refresh().catch((cause) => {
          if (generation === session) {
            error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
          }
        });
      }
    }, 2000);
    return () => {
      session++;
      columnRequest++;
      clearInterval(timer);
    };
  });
  async function chooseBoard(value: string, preserve = false) {
    boardId = value;
    const request = ++columnRequest;
    columns = [];
    if (editing) {
      for (const field of editing.settings) {
        if (field.type === "board") {
          config[field.key] = Number(value);
        }
        if (field.type === "column" && !preserve) {
          delete config[field.key];
        }
      }
    }
    if (!value) {
      return;
    }
    try {
      const result = await invoke<{ id: number; name: string }[]>(
        "get_board_columns",
        { boardId: Number(value) },
      );
      if (request === columnRequest) {
        columns = result;
      }
    } catch (cause) {
      if (request === columnRequest) {
        error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
      }
    }
  }
  function edit(pkg: PluginPackage, instance?: PluginInstance) {
    editing = pkg;
    instanceId = instance?.id;
    name = instance?.name ?? pluginText(pkg.name);
    config = instance ? { ...instance.config } : pluginDefaults(pkg);
    secrets = {};
    savedSecrets = instance?.secret_fields ?? [];
    domains = [...(instance?.allowed_domains ?? [])];
    enabled = instance?.enabled ?? false;
    interval = instance?.interval_seconds ?? 0;
    consent = false;
    invalid = [];
    historyId = null;
    error = null;
    void chooseBoard(instance ? String(instance.board_id) : "", true);
  }
  async function save() {
    if (!editing) {
      return;
    }
    invalid = invalidPluginSettings(
      editing.settings,
      config,
      secrets,
      savedSecrets,
    );
    if (
      !name.trim() ||
      !boardId ||
      !consent ||
      interval === undefined ||
      !Number.isSafeInteger(interval) ||
      (interval !== 0 && interval < 60) ||
      invalid.length
    ) {
      error = { code: "INVALID_ARGUMENT" };
      return;
    }
    const pkg = editing;
    await action(async () => {
      await invoke("save_plugin_instance", {
        instance: {
          id: instanceId ?? null,
          plugin_id: pkg.id,
          name: name.trim(),
          board_id: Number(boardId),
          config,
          allowed_domains: domains,
          enabled,
          interval_seconds: interval,
        },
        secrets: Object.fromEntries(
          Object.entries(secrets).filter(([, value]) => value !== ""),
        ),
      });
      secrets = {};
      editing = null;
    });
  }
  async function install() {
    await action(async () => {
      const path = await openFile({
        multiple: false,
        filters: [{ name: m.plugin_manifest(), extensions: ["json"] }],
      });
      if (typeof path === "string") {
        await invoke("install_plugin_package", { path });
      }
    });
  }
  async function run(instance: PluginInstance) {
    const generation = session;
    error = null;
    instance.running = true;
    try {
      await invoke("run_plugin_instance", { instanceId: instance.id });
    } catch (cause) {
      if (generation === session) {
        error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
      }
    } finally {
      if (generation === session) {
        await refresh().catch((cause) => {
          if (generation === session) {
            error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
          }
        });
      }
    }
  }
  async function remove() {
    if (!removal) {
      return;
    }
    const target = removal;
    await action(async () => {
      await invoke(
        target.kind === "package"
          ? "remove_plugin_package"
          : "remove_plugin_instance",
        target.kind === "package"
          ? { pluginId: target.id }
          : { instanceId: target.id },
      );
      removal = null;
      editing = null;
      secrets = {};
      historyId = null;
    });
  }
  function status(value: PluginRun["status"]) {
    return {
      running: m.plugin_running(),
      success: m.plugin_success(),
      failed: m.plugin_failed(),
      cancelled: m.plugin_cancelled(),
      interrupted: m.plugin_interrupted(),
    }[value];
  }
</script>

{#snippet picker(
  id: string,
  value: string,
  options: { value: string; label: string }[],
  change: (value: string) => void,
)}
  <Select.Root type="single" {value} onValueChange={change} disabled={pending}>
    <Select.Trigger {id} class="w-full"
      >{options.find((option) => option.value === value)?.label ??
        m.plugin_select()}</Select.Trigger
    >
    <Select.Content
      >{#each options as option (option.value)}<Select.Item value={option.value}
          >{option.label}</Select.Item
        >{/each}</Select.Content
    >
  </Select.Root>
{/snippet}

<Dialog.Root bind:open>
  <Dialog.Content class="flex max-h-[90dvh] flex-col sm:max-w-3xl">
    <Dialog.Header
      ><Dialog.Title>{m.plugin_title()}</Dialog.Title><Dialog.Description
        >{m.plugin_description()}</Dialog.Description
      ></Dialog.Header
    >
    <div class="min-h-0 space-y-4 overflow-y-auto p-1">
      {#if error}<p role="alert" class="text-sm text-destructive">
          {pluginErrorMessage(error)}
        </p>{/if}
      <div class="flex flex-wrap items-center gap-3">
        <Button variant="outline" disabled={pending} onclick={install}
          >{m.plugin_install()}</Button
        >
        <Button
          variant="ghost"
          disabled={pending}
          onclick={() => void action(refresh)}>{m.plugin_refresh()}</Button
        >
        <div class="flex items-center gap-2">
          <Switch
            id="plugin-safe-mode"
            checked={pluginState.safe_mode}
            disabled={pending}
            onCheckedChange={(enabled) =>
              void action(() => invoke("set_plugin_safe_mode", { enabled }))}
          /><Label for="plugin-safe-mode">{m.plugin_safe_mode()}</Label>
        </div>
      </div>
      {#if pluginState.safe_mode}<p
          class="rounded-md border bg-muted p-3 text-sm"
        >
          {m.plugin_safe_mode_help()}
        </p>{/if}
      {#if removal}
        <div role="alert" class="space-y-2 rounded-md border p-3">
          <p class="text-sm">{m.plugin_remove_help()}</p>
          <div class="flex gap-2">
            <Button variant="destructive" disabled={pending} onclick={remove}
              >{m.common_delete()}</Button
            ><Button
              variant="outline"
              disabled={pending}
              onclick={() => (removal = null)}>{m.common_cancel()}</Button
            >
          </div>
        </div>
      {/if}
      {#if loaded && !pluginState.packages.length}<p
          class="text-sm text-muted-foreground"
        >
          {m.plugin_empty()}
        </p>{/if}
      {#each pluginState.packages as pkg (pkg.id)}
        <section
          class="space-y-3 rounded-lg border p-4"
          aria-label={pluginText(pkg.name)}
        >
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div>
              <h3 class="font-semibold">
                {pluginText(pkg.name)}
                <span class="text-sm font-normal text-muted-foreground"
                  >{pkg.version}</span
                >
              </h3>
              <p class="text-sm text-muted-foreground">
                {pluginText(pkg.description)}
              </p>
            </div>
            <div class="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={pending}
                onclick={() => edit(pkg)}>{m.plugin_add_instance()}</Button
              ><Button
                size="sm"
                variant="ghost"
                disabled={pending}
                onclick={() => (removal = { kind: "package", id: pkg.id })}
                >{m.common_delete()}</Button
              >
            </div>
          </div>
          {#each pluginState.instances.filter((instance) => instance.plugin_id === pkg.id) as instance (instance.id)}
            <div class="space-y-2 border-t pt-3">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <h4 class="font-medium">{instance.name}</h4>
                <div class="flex items-center gap-2">
                  <Switch
                    id={"plugin-enable-" + instance.id}
                    checked={instance.enabled}
                    disabled={pending}
                    onCheckedChange={(enabled) =>
                      void action(() =>
                        invoke("set_plugin_enabled", {
                          instanceId: instance.id,
                          enabled,
                        }),
                      )}
                  /><Label for={"plugin-enable-" + instance.id}
                    >{m.plugin_enabled()}</Label
                  >
                </div>
              </div>
              <p class="text-sm text-muted-foreground">
                {m.plugin_last_run({
                  time: instance.last_run_at
                    ? formatDateTime(instance.last_run_at)
                    : m.plugin_never(),
                })}
              </p>
              <p class="text-sm text-muted-foreground">
                {instance.interval_seconds
                  ? m.plugin_schedule_summary({
                      seconds: formatNumber(instance.interval_seconds),
                      time: instance.next_run_at
                        ? formatDateTime(instance.next_run_at)
                        : m.plugin_never(),
                    })
                  : m.plugin_manual_only()}
              </p>
              {#if instance.last_run_status && !instance.running}<p
                  class="text-sm"
                >
                  {status(instance.last_run_status)}
                </p>{/if}
              {#if instance.running}<p role="status" class="text-sm">
                  {m.plugin_running()}
                </p>{/if}
              {#if instance.last_error}<p
                  role="status"
                  class="text-sm text-destructive"
                >
                  {pluginErrorMessage(instance.last_error)}
                </p>{/if}
              <div class="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  disabled={pending ||
                    !instance.enabled ||
                    instance.running ||
                    pluginState.safe_mode}
                  onclick={() => void run(instance)}>{m.plugin_run()}</Button
                >
                {#if instance.running}<Button
                    size="sm"
                    variant="outline"
                    onclick={() =>
                      void invoke("cancel_plugin_run", {
                        instanceId: instance.id,
                      }).catch((cause) => {
                        error = parseCommandError(cause) ?? {
                          code: "INTERNAL_ERROR",
                        };
                      })}>{m.plugin_cancel_run()}</Button
                  >{/if}
                <Button
                  size="sm"
                  variant="outline"
                  disabled={pending}
                  onclick={() => edit(pkg, instance)}
                  >{m.settings_title()}</Button
                >
                <Button
                  size="sm"
                  variant="outline"
                  disabled={pending}
                  onclick={() => {
                    historyId = instance.id;
                    editing = null;
                    secrets = {};
                    void action(refresh);
                  }}>{m.plugin_logs()}</Button
                >
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={pending}
                  onclick={() =>
                    (removal = { kind: "instance", id: instance.id })}
                  >{m.common_delete()}</Button
                >
              </div>
            </div>
          {/each}
        </section>
      {/each}
      {#if editing}
        <form
          class="space-y-4 rounded-lg border p-4"
          onsubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <h3 class="font-semibold">
            {pluginText(editing.name)} — {m.settings_title()}
          </h3>
          <div class="grid gap-2">
            <Label for="plugin-name">{m.common_name()}</Label><Input
              id="plugin-name"
              bind:value={name}
              required
              disabled={pending}
            />
          </div>
          <div class="grid gap-2">
            <Label for="plugin-board">{m.plugin_target_board()}</Label
            >{@render picker(
              "plugin-board",
              boardId,
              board.boards
                .filter((item) => item.shared_role !== "viewer")
                .map((item) => ({ value: String(item.id), label: item.name })),
              (value) => {
                consent = false;
                void chooseBoard(value);
              },
            )}
          </div>
          {#each editing.settings as field (field.key)}
            {#if field.type !== "board"}
              <div class="grid gap-2">
                <Label for={"plugin-field-" + field.key}
                  >{pluginText(field.label)}{field.required ? " *" : ""}</Label
                >
                {#if field.type === "boolean"}<Switch
                    id={"plugin-field-" + field.key}
                    aria-invalid={invalid.includes(field.key)}
                    aria-describedby={invalid.includes(field.key)
                      ? "plugin-invalid-" + field.key
                      : undefined}
                    checked={config[field.key] === true}
                    onCheckedChange={(value) => (config[field.key] = value)}
                    disabled={pending}
                  />
                {:else if field.type === "select" || field.type === "column"}
                  {@render picker(
                    "plugin-field-" + field.key,
                    String(config[field.key] ?? ""),
                    field.type === "column"
                      ? columns.map((column) => ({
                          value: String(column.id),
                          label: column.name,
                        }))
                      : (field.options ?? []).map((option) => ({
                          value: option.value,
                          label: pluginText(option.label),
                        })),
                    (value) =>
                      (config[field.key] =
                        field.type === "column" ? Number(value) : value),
                  )}
                {:else if field.type === "secret"}
                  <Input
                    id={"plugin-field-" + field.key}
                    aria-invalid={invalid.includes(field.key)}
                    aria-describedby={invalid.includes(field.key)
                      ? "plugin-invalid-" + field.key
                      : undefined}
                    type="password"
                    autocomplete="new-password"
                    value={secrets[field.key] ?? ""}
                    oninput={(event) =>
                      (secrets[field.key] = event.currentTarget.value)}
                    disabled={pending}
                  />
                  <p class="text-xs text-muted-foreground">
                    {savedSecrets.includes(field.key)
                      ? m.plugin_secret_saved()
                      : m.plugin_secret_help()}
                  </p>
                {:else if field.type === "number"}
                  <Input
                    id={"plugin-field-" + field.key}
                    aria-invalid={invalid.includes(field.key)}
                    aria-describedby={invalid.includes(field.key)
                      ? "plugin-invalid-" + field.key
                      : undefined}
                    type="number"
                    min={field.min ?? undefined}
                    max={field.max ?? undefined}
                    step="any"
                    value={typeof config[field.key] === "number"
                      ? (config[field.key] as number)
                      : undefined}
                    oninput={(event) =>
                      (config[field.key] =
                        event.currentTarget.value === ""
                          ? undefined
                          : event.currentTarget.valueAsNumber)}
                    disabled={pending}
                  />
                {:else}<Input
                    id={"plugin-field-" + field.key}
                    aria-invalid={invalid.includes(field.key)}
                    aria-describedby={invalid.includes(field.key)
                      ? "plugin-invalid-" + field.key
                      : undefined}
                    value={String(config[field.key] ?? "")}
                    oninput={(event) =>
                      (config[field.key] = event.currentTarget.value)}
                    required={field.required}
                    disabled={pending}
                  />{/if}
                {#if invalid.includes(field.key)}<p
                    id={"plugin-invalid-" + field.key}
                    class="text-sm text-destructive"
                  >
                    {m.plugin_field_invalid()}
                  </p>{/if}
              </div>
            {/if}
          {/each}
          <div class="grid gap-2">
            <Label for="plugin-interval">{m.plugin_interval()}</Label><Input
              id="plugin-interval"
              type="number"
              min="0"
              step="1"
              bind:value={interval}
              disabled={pending}
            />
            <p class="text-xs text-muted-foreground">
              {m.plugin_interval_help({ seconds: formatNumber(60) })}
            </p>
          </div>
          <fieldset class="space-y-3 rounded-md border p-3">
            <legend class="text-sm font-medium">{m.plugin_permissions()}</legend
            >
            <p class="text-sm">{m.plugin_permissions_help()}</p>
            {#each editing.domains as domain (domain)}<div
                class="flex items-center gap-2"
              >
                <Checkbox
                  id={"plugin-domain-" + domain}
                  checked={domains.includes(domain)}
                  disabled={pending}
                  onCheckedChange={(checked) => {
                    domains = checked
                      ? [...domains, domain]
                      : domains.filter((value) => value !== domain);
                    consent = false;
                  }}
                /><Label for={"plugin-domain-" + domain}>{domain}</Label>
              </div>{/each}
            <div class="flex items-center gap-2">
              <Checkbox
                id="plugin-consent"
                bind:checked={consent}
                disabled={pending}
              /><Label for="plugin-consent">{m.plugin_grant_board()}</Label>
            </div>
          </fieldset>
          <div class="flex items-center gap-2">
            <Switch
              id="plugin-edit-enabled"
              bind:checked={enabled}
              disabled={pending}
            /><Label for="plugin-edit-enabled">{m.plugin_enabled()}</Label>
          </div>
          <div class="flex gap-2">
            <Button type="submit" disabled={pending}>{m.common_save()}</Button
            ><Button
              variant="outline"
              disabled={pending}
              onclick={() => {
                editing = null;
                secrets = {};
              }}>{m.common_cancel()}</Button
            >
          </div>
        </form>
      {/if}
      {#if historyId}
        <section
          class="space-y-3 rounded-lg border p-4"
          aria-label={m.plugin_logs()}
        >
          <h3 class="font-semibold">{m.plugin_logs()}</h3>
          {#if !runs.length}<p class="text-sm text-muted-foreground">
              {m.plugin_no_runs()}
            </p>{/if}
          {#each runs as run (run.id)}<div class="space-y-2 border-t pt-3">
              <p class="text-sm font-medium">
                {formatDateTime(run.started_at)} · {status(run.status)} · {run.trigger ===
                "manual"
                  ? m.plugin_manual_trigger()
                  : m.plugin_schedule_trigger()}
              </p>
              <p class="text-sm">
                {m.plugin_run_counts({
                  created: formatNumber(run.created),
                  updated: formatNumber(run.updated),
                })}
              </p>
              {#if run.finished_at}<p class="text-xs text-muted-foreground">
                  {m.plugin_duration({
                    seconds: formatNumber(
                      (run.finished_at - run.started_at) / 1000,
                      { maximumFractionDigits: 1 },
                    ),
                  })}
                </p>{/if}{#if run.error}<p class="text-sm text-destructive">
                  {pluginErrorMessage(run.error)}
                </p>{/if}
              <p class="break-all font-mono text-xs text-muted-foreground">
                {run.id}
              </p>
              {#each run.logs as log, i (i)}<p
                  class="whitespace-pre-wrap break-words font-mono text-xs"
                >
                  {pluginLogMessage(log)}
                </p>{/each}
            </div>{/each}
        </section>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
