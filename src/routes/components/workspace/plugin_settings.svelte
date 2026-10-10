<script lang="ts">
  import { tick, untrack } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import {
    getCurrentWebview,
    type DragDropEvent,
  } from "@tauri-apps/api/webview";
  import { open as openFile } from "@tauri-apps/plugin-dialog";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Tabs } from "bits-ui";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";
  import FileArchiveIcon from "@lucide/svelte/icons/file-archive";
  import LinkIcon from "@lucide/svelte/icons/link";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
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
  import { board, type BoardSummary } from "../../board.svelte";

  let { open = $bindable(false) }: { open: boolean } = $props();
  let pluginState = $state<PluginState>({
    packages: [],
    instances: [],
    safe_mode: false,
  });
  let error = $state<CommandError | null>(null);
  let pending = $state(false);
  let packageUrl = $state("");
  let dropZone = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);
  let dialogContent = $state<HTMLDivElement | null>(null);
  let installation = $state<
    | { status: "installing"; source: string | null }
    | { status: "failed"; source: string | null; error: CommandError }
    | { status: "success"; source: string | null; package: PluginPackage }
    | null
  >(null);

  let loaded = $state(false);
  let editing = $state<PluginPackage | null>(null);
  let instanceId = $state<string | undefined>();
  let name = $state("");
  let boardId = $state("");
  let newColumnField = $state<string | null>(null);
  let newColumnName = $state("");
  let creatingColumn = $state(false);
  let newColumnError = $state<CommandError | null>(null);
  let newBoardOpen = $state(false);
  let newBoardName = $state("");
  let creatingBoard = $state(false);
  let newBoardError = $state<CommandError | null>(null);
  let config = $state<Record<string, unknown>>({});
  let secrets = $state<Record<string, string>>({});
  let savedSecrets = $state<string[]>([]);
  let domains = $state<string[]>([]);
  let enabled = $state(false);
  let interval = $state<number | undefined>(0);
  let consent = $state(false);
  let invalid = $state<string[]>([]);
  let saveAttempted = $state(false);
  let editorForm = $state<HTMLFormElement | null>(null);
  const invalidInterval = $derived(
    interval === undefined ||
      !Number.isSafeInteger(interval) ||
      (interval !== 0 && interval < 60),
  );
  let columns = $state<{ id: number; name: string }[]>([]);
  let columnsLoading = $state(false);
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
      closeEditor();
      historyId = null;
      runs = [];
      installation = null;
      return;
    }
    untrack(() => {
      void action(refresh);
      if (editing) {
        void refreshColumns();
      }
    });
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
    await refreshColumns();
  }
  async function refreshColumns() {
    const value = boardId;
    const request = ++columnRequest;
    if (!value) {
      columnsLoading = false;
      return;
    }
    columnsLoading = true;
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
    } finally {
      if (request === columnRequest) {
        columnsLoading = false;
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
    saveAttempted = false;
    historyId = null;
    error = null;
    void chooseBoard(instance ? String(instance.board_id) : "", true);
  }
  async function focusEditorError() {
    await tick();
    const target = editorForm?.querySelector<HTMLElement>(
      '[data-plugin-error], [aria-invalid="true"]:not(:disabled), [data-invalid="true"] button:not(:disabled), [data-invalid="true"] input:not(:disabled)',
    );
    target?.focus({ preventScroll: true });
    target?.scrollIntoView({ block: "center", inline: "nearest" });
  }
  async function save() {
    if (!editing) {
      return;
    }
    saveAttempted = true;
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
      invalidInterval ||
      invalid.length
    ) {
      error = null;
      await focusEditorError();
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
    if (editing && error) {
      await focusEditorError();
    }
  }
  async function install() {
    await action(async () => {
      const path = await openFile({
        multiple: false,
        filters: [
          { name: m.plugin_package_file(), extensions: ["zip", "json"] },
        ],
      });
      if (typeof path === "string") {
        await installPackage({ path });
      }
    });
  }
  async function installPackage(input: { path: string } | { url: string }) {
    const generation = session;
    const source =
      "path" in input ? (input.path.split(/[\\/]/).at(-1) ?? input.path) : null;
    installation = { status: "installing", source };
    try {
      const pkg = await invoke<PluginPackage>("install_plugin_package", input);
      if (generation === session) {
        installation = { status: "success", source, package: pkg };
        return pkg;
      }
    } catch (cause) {
      if (generation === session) {
        installation = {
          status: "failed",
          source,
          error: parseCommandError(cause) ?? { code: "INTERNAL_ERROR" },
        };
      }
    }
  }
  async function handlePluginDrop(event: DragDropEvent) {
    if (!open || pending || editing || !dropZone || event.type === "leave") {
      dragging = false;
      return;
    }
    const { x, y } = event.position.toLogical(window.devicePixelRatio);
    const bounds = dropZone.getBoundingClientRect();
    const viewport = dropZone
      .closest('[data-slot="scroll-area-viewport"]')
      ?.getBoundingClientRect();
    const inside =
      !!viewport &&
      x >= bounds.left &&
      x <= bounds.right &&
      y >= bounds.top &&
      y <= bounds.bottom &&
      x >= viewport.left &&
      x <= viewport.right &&
      y >= viewport.top &&
      y <= viewport.bottom;
    dragging = inside && event.type !== "drop";
    if (!inside || event.type !== "drop") {
      return;
    }
    const path = event.paths[0];
    if (
      event.paths.length !== 1 ||
      !/(?:\.zip|(?:^|[\\/])manifest\.json)$/i.test(path)
    ) {
      installation = {
        status: "failed",
        source: null,
        error: { code: "INVALID_ARGUMENT" },
      };
      return;
    }
    await action(() => installPackage({ path }));
  }
  function preventFileNavigation(event: DragEvent) {
    if (open && event.dataTransfer?.types.includes("Files")) {
      event.preventDefault();
    }
  }
  $effect(() => {
    if (!open || !isTauri()) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (!disposed) {
          void handlePluginDrop(event.payload);
        }
      })
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch((cause) => {
        if (!disposed) {
          error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
        }
      });
    return () => {
      disposed = true;
      dragging = false;
      unlisten?.();
    };
  });
  async function showHistory(id: string) {
    historyId = id;
    runs = [];
    editing = null;
    secrets = {};
    await action(async () => {
      await refreshRequest;
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
  function closeEditor() {
    editing = null;
    secrets = {};
    newBoardOpen = false;
    newColumnField = null;
    columnRequest++;
  }
  async function createColumn() {
    const name = newColumnName.trim();
    const target = boardId;
    const field = newColumnField;
    if (!name || !target || !field || creatingColumn || pending) {
      return;
    }
    const generation = session;
    creatingColumn = true;
    newColumnError = null;
    try {
      const id = await invoke<number>("add_column_to_board", {
        boardId: Number(target),
        name,
      });
      await board.get_boards();
      if (board.active_board_id === Number(target)) {
        board.get_columns();
      }
      if (generation === session && editing && boardId === target) {
        columns = [...columns, { id, name }];
        config[field] = id;
        newColumnField = null;
      }
    } catch (cause) {
      if (generation === session) {
        newColumnError = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
      }
    } finally {
      creatingColumn = false;
    }
  }
  async function createBoard() {
    const name = newBoardName.trim();
    if (!name || creatingBoard || pending) {
      return;
    }
    const generation = session;
    creatingBoard = true;
    newBoardError = null;
    try {
      const created = await invoke<BoardSummary>("create_board", { name });
      board.boards = [...board.boards, created];
      if (generation === session && editing) {
        consent = false;
        await chooseBoard(String(created.id));
        newBoardOpen = false;
      }
    } catch (cause) {
      if (generation === session) {
        newBoardError = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
      }
    } finally {
      creatingBoard = false;
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
  refreshOptions?: () => Promise<void>,
)}
  <Select.Root
    type="single"
    {value}
    onValueChange={change}
    onOpenChange={(open) => {
      if (open) {
        void refreshOptions?.();
      }
    }}
    disabled={pending || !options.length}
  >
    <Select.Trigger
      {id}
      class="w-full"
      aria-invalid={saveAttempted &&
        (id === "plugin-board"
          ? !boardId
          : invalid.some((key) => id === "plugin-field-" + key))}
      ><span class="truncate"
        >{options.find((option) => option.value === value)?.label ??
          m.plugin_select()}</span
      ></Select.Trigger
    >
    <Select.Content
      >{#each options as option (option.value)}<Select.Item value={option.value}
          >{option.label}</Select.Item
        >{/each}</Select.Content
    >
  </Select.Root>
{/snippet}

<svelte:window
  ondragover={preventFileNavigation}
  ondrop={preventFileNavigation}
/>

<Dialog.Root bind:open>
  <Dialog.Content
    bind:ref={dialogContent}
    tabindex={-1}
    class="flex h-[85dvh] max-h-200 flex-col overflow-hidden outline-none sm:max-w-2xl"
    onOpenAutoFocus={(event) => {
      event.preventDefault();
      dialogContent?.focus();
    }}
  >
    <Dialog.Header
      ><Dialog.Title>{m.plugin_title()}</Dialog.Title><Dialog.Description
        >{m.plugin_description()}</Dialog.Description
      ></Dialog.Header
    >
    <ScrollArea
      type="always"
      class="min-h-0 flex-1 [overflow-anchor:none]"
      scrollbarYClasses="[&_[data-slot=scroll-area-thumb]]:bg-muted-foreground/50"
    >
      <div class="space-y-6 py-1 ps-1 pe-4">
        {#if error && !editing}<p role="alert" class="text-sm text-destructive">
            {pluginErrorMessage(error)}
          </p>{/if}
        <section class="space-y-3" aria-label={m.plugin_install()}>
          <h3 class="text-sm font-medium">{m.plugin_install()}</h3>
          <Tabs.Root value="local" class="space-y-4">
            <Tabs.List
              aria-label={m.plugin_install_source()}
              class="grid h-9 w-full grid-cols-2 items-center rounded-lg bg-muted p-1 text-muted-foreground"
            >
              <Tabs.Trigger
                value="local"
                class="inline-flex h-7 items-center justify-center gap-2 rounded-md px-3 text-sm font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm"
              >
                <FileArchiveIcon class="size-4" />{m.plugin_source_local()}
              </Tabs.Trigger>
              <Tabs.Trigger
                value="url"
                class="inline-flex h-7 items-center justify-center gap-2 rounded-md px-3 text-sm font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm"
              >
                <LinkIcon class="size-4" />{m.plugin_source_url()}
              </Tabs.Trigger>
            </Tabs.List>
            <Tabs.Content
              value="local"
              class="outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg"
            >
              <div bind:this={dropZone}>
                <Empty.Root
                  class={dragging
                    ? "gap-4 border border-primary bg-accent py-6 md:p-6"
                    : "gap-4 border border-input bg-muted/20 py-6 md:p-6"}
                >
                  <Empty.Header>
                    <Empty.Media variant="icon"><FileArchiveIcon /></Empty.Media
                    >
                    <Empty.Title class="text-sm"
                      >{m.plugin_drop_file()}</Empty.Title
                    >
                    <Empty.Description class="text-xs"
                      >{m.plugin_local_help()}</Empty.Description
                    >
                  </Empty.Header>
                  <Button disabled={pending} onclick={install}
                    >{installation?.status === "installing"
                      ? m.plugin_installing()
                      : m.plugin_choose_file()}</Button
                  >
                </Empty.Root>
              </div>
            </Tabs.Content>
            <Tabs.Content
              value="url"
              class="outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-lg"
            >
              <form
                novalidate
                class="space-y-4 rounded-lg border border-input p-5"
                onsubmit={(event) => {
                  event.preventDefault();
                  if (packageUrl.trim()) {
                    void action(async () => {
                      const pkg = await installPackage({
                        url: packageUrl.trim(),
                      });
                      if (pkg) {
                        packageUrl = "";
                      }
                    });
                  }
                }}
              >
                <div class="space-y-2">
                  <Label for="plugin-package-url"
                    >{m.plugin_package_url()}</Label
                  >
                  <Input
                    id="plugin-package-url"
                    type="url"
                    bind:value={packageUrl}
                    placeholder="https://github.com/owner/repo/releases/download/v1.0/plugin.zip"
                    required
                    disabled={pending}
                    aria-describedby="plugin-package-help"
                  />
                  <p
                    id="plugin-package-help"
                    class="text-xs leading-relaxed text-muted-foreground"
                  >
                    {m.plugin_package_help()}
                  </p>
                </div>
                <div class="flex justify-end">
                  <Button type="submit" disabled={pending || !packageUrl.trim()}
                    >{installation?.status === "installing"
                      ? m.plugin_installing()
                      : m.plugin_install_url()}</Button
                  >
                </div>
              </form>
            </Tabs.Content>
          </Tabs.Root>
          {#if installation}
            <div
              role="status"
              aria-live="polite"
              class="flex items-start gap-3 rounded-lg border bg-muted/30 p-4"
            >
              {#if installation.status === "installing"}
                <Spinner class="mt-0.5 size-5 shrink-0" aria-hidden="true" />
              {:else if installation.status === "success"}
                <CircleCheckIcon class="mt-0.5 size-5 shrink-0 text-success" />
              {:else}
                <CircleAlertIcon
                  class="mt-0.5 size-5 shrink-0 text-destructive"
                />
              {/if}
              <div class="min-w-0 space-y-1">
                <p class="text-sm font-medium">
                  {installation.status === "installing"
                    ? m.plugin_installing()
                    : installation.status === "success"
                      ? m.plugin_install_success({
                          name: pluginText(installation.package.name),
                        })
                      : m.plugin_install_failed()}
                </p>
                {#if installation.source}<p
                    class="break-all text-xs text-muted-foreground"
                  >
                    {installation.source}
                  </p>{/if}
                {#if installation.status === "failed"}<p
                    class="text-sm text-destructive"
                  >
                    {pluginErrorMessage(installation.error)}
                  </p>{/if}
              </div>
            </div>
          {/if}
        </section>
        <div class="flex items-center justify-between border-t pt-5">
          <h3 class="text-sm font-medium">{m.plugin_installed_packages()}</h3>
          <Button
            size="sm"
            variant="ghost"
            disabled={pending}
            onclick={() => void action(refresh)}
            ><RefreshCwIcon />{m.plugin_refresh()}</Button
          >
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
                  variant="destructive"
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
                    onclick={() => void showHistory(instance.id)}
                    >{m.plugin_logs()}</Button
                  >
                  <Button
                    size="sm"
                    variant="destructive"
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
        <div class="flex items-center justify-between gap-4 border-t pt-4">
          <Label for="plugin-safe-mode" class="text-sm text-muted-foreground"
            >{m.plugin_safe_mode()}</Label
          >
          <Switch
            id="plugin-safe-mode"
            checked={pluginState.safe_mode}
            disabled={pending}
            onCheckedChange={(enabled) =>
              void action(() => invoke("set_plugin_safe_mode", { enabled }))}
          />
        </div>
      </div>
    </ScrollArea>
  </Dialog.Content>
  <Dialog.Root
    bind:open={
      () => open && historyId !== null,
      (value) => {
        if (!value) historyId = null;
      }
    }
  >
    <Dialog.Content
      class="flex h-[85dvh] max-h-200 flex-col overflow-hidden sm:max-w-2xl"
    >
      <Dialog.Header>
        <Dialog.Title>{m.plugin_logs()}</Dialog.Title>
        <Dialog.Description
          >{pluginState.instances.find((instance) => instance.id === historyId)
            ?.name ?? m.plugin_logs()}</Dialog.Description
        >
      </Dialog.Header>
      <ScrollArea type="always" class="min-h-0 flex-1">
        <div class="space-y-3 py-1 ps-1 pe-4">
          {#if error}<p role="alert" class="text-sm text-destructive">
              {pluginErrorMessage(error)}
            </p>{/if}
          {#if pending && !runs.length}<p
              role="status"
              class="text-sm text-muted-foreground"
            >
              {m.ui_loading()}
            </p>
          {:else if !runs.length}<p class="text-sm text-muted-foreground">
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
        </div>
      </ScrollArea>
    </Dialog.Content>
  </Dialog.Root>
</Dialog.Root>

{#if editing}
  <Dialog.Root
    bind:open={
      () => open && editing !== null,
      (value) => {
        if (!value) closeEditor();
      }
    }
    busy={pending || creatingBoard || creatingColumn}
  >
    <Dialog.Content
      class="flex h-[85dvh] max-h-200 flex-col overflow-hidden sm:max-w-lg"
    >
      <Dialog.Header class="shrink-0">
        <Dialog.Title
          >{instanceId
            ? m.settings_title()
            : m.plugin_add_instance()}</Dialog.Title
        >
        <Dialog.Description>{pluginText(editing.name)}</Dialog.Description>
      </Dialog.Header>
      <form
        novalidate
        bind:this={editorForm}
        class="flex min-h-0 flex-1 flex-col gap-4"
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <ScrollArea
          type="always"
          class="min-h-0 flex-1"
          scrollbarYClasses="[&_[data-slot=scroll-area-thumb]]:bg-muted-foreground/50"
        >
          <Field.Group class="py-1 ps-1 pe-4">
            {#if error}<p
                role="alert"
                data-plugin-error
                tabindex="-1"
                class="text-sm text-destructive"
              >
                {pluginErrorMessage(error)}
              </p>{/if}

            <Field.Field
              data-disabled={pending}
              data-invalid={saveAttempted && !name.trim()}
            >
              <Field.Label for="plugin-name">{m.common_name()}</Field.Label
              ><Input
                id="plugin-name"
                aria-invalid={saveAttempted && !name.trim()}
                aria-describedby={saveAttempted && !name.trim()
                  ? "plugin-name-error"
                  : undefined}
                bind:value={name}
                required
                disabled={pending}
              />
              {#if saveAttempted && !name.trim()}<Field.Error
                  id="plugin-name-error">{m.plugin_name_required()}</Field.Error
                >{/if}
            </Field.Field>
            <Field.Field
              data-disabled={pending}
              data-invalid={saveAttempted && !boardId}
            >
              <Field.Label for="plugin-board"
                >{m.plugin_target_board()}</Field.Label
              >
              <div class="flex min-w-0 items-center gap-2">
                <div class="min-w-0 flex-1">
                  {@render picker(
                    "plugin-board",
                    boardId,
                    board.boards
                      .filter((item) => item.shared_role !== "viewer")
                      .map((item) => ({
                        value: String(item.id),
                        label: item.name,
                      })),
                    (value) => {
                      consent = false;
                      void chooseBoard(value);
                    },
                  )}
                </div>
                <Button
                  variant="outline"
                  disabled={pending || creatingBoard}
                  onclick={() => {
                    newBoardName = "";
                    newBoardError = null;
                    newBoardOpen = true;
                  }}><PlusIcon />{m.board_new()}</Button
                >
              </div>
            </Field.Field>
            {#if saveAttempted && !boardId}<Field.Error
                >{m.plugin_choose_board_first()}</Field.Error
              >{/if}
            {#each editing.settings as field (field.key)}
              {#if field.type !== "board"}
                <Field.Field
                  data-disabled={pending}
                  data-invalid={invalid.includes(field.key)}
                  orientation={field.type === "boolean"
                    ? "horizontal"
                    : "vertical"}
                  class={field.type === "boolean" ? "flex-wrap" : undefined}
                >
                  <Field.Label for={"plugin-field-" + field.key}
                    >{pluginText(field.label)}{field.required
                      ? " *"
                      : ""}</Field.Label
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
                  {:else if field.type === "column"}
                    <div class="flex items-center gap-2">
                      <div class="min-w-0 flex-1">
                        {#if !columns.length}
                          <Input
                            id={"plugin-field-" + field.key}
                            value={columnsLoading
                              ? m.ui_loading()
                              : boardId
                                ? m.ui_no_columns_yet()
                                : m.plugin_choose_board_first()}
                            disabled
                          />
                        {:else}
                          {@render picker(
                            "plugin-field-" + field.key,
                            String(config[field.key] ?? ""),
                            columns.map((column) => ({
                              value: String(column.id),
                              label: column.name,
                            })),
                            (value) => (config[field.key] = Number(value)),
                            refreshColumns,
                          )}
                        {/if}
                      </div>
                      <Button
                        variant="outline"
                        disabled={pending ||
                          !boardId ||
                          (columnsLoading && !columns.length)}
                        onclick={() => {
                          newColumnName = "";
                          newColumnError = null;
                          newColumnField = field.key;
                        }}
                        ><PlusIcon aria-hidden="true" />{m.column_new()}</Button
                      >
                    </div>
                  {:else if field.type === "select"}
                    {@render picker(
                      "plugin-field-" + field.key,
                      String(config[field.key] ?? ""),
                      (field.options ?? []).map((option) => ({
                        value: option.value,
                        label: pluginText(option.label),
                      })),
                      (value) => (config[field.key] = value),
                    )}
                  {:else if field.type === "secret"}
                    <Input
                      id={"plugin-field-" + field.key}
                      aria-invalid={invalid.includes(field.key)}
                      aria-describedby={"plugin-help-" +
                        field.key +
                        (invalid.includes(field.key)
                          ? " plugin-invalid-" + field.key
                          : "")}
                      type="password"
                      autocomplete="new-password"
                      value={secrets[field.key] ?? ""}
                      oninput={(event) =>
                        (secrets[field.key] = event.currentTarget.value)}
                      disabled={pending}
                    />
                    <Field.Description id={"plugin-help-" + field.key}>
                      {savedSecrets.includes(field.key)
                        ? m.plugin_secret_saved()
                        : m.plugin_secret_help()}
                    </Field.Description>
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
                      autocomplete="off"
                      value={String(config[field.key] ?? "")}
                      oninput={(event) =>
                        (config[field.key] = event.currentTarget.value)}
                      required={field.required}
                      disabled={pending}
                    />{/if}
                  {#if invalid.includes(field.key)}<Field.Error
                      id={"plugin-invalid-" + field.key}
                      class="w-full"
                    >
                      {m.plugin_field_invalid()}
                    </Field.Error>{/if}
                </Field.Field>
              {/if}
            {/each}
            <Field.Field
              data-disabled={pending}
              data-invalid={saveAttempted && invalidInterval}
            >
              <Field.Label for="plugin-interval"
                >{m.plugin_interval()}</Field.Label
              ><Input
                id="plugin-interval"
                aria-invalid={saveAttempted && invalidInterval}
                type="number"
                class="[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
                min="0"
                step="1"
                bind:value={interval}
                aria-describedby={saveAttempted && invalidInterval
                  ? "plugin-interval-help plugin-interval-error"
                  : "plugin-interval-help"}
                disabled={pending}
              />
              {#if saveAttempted && invalidInterval}<Field.Error
                  id="plugin-interval-error"
                  >{m.plugin_field_invalid()}</Field.Error
                >{/if}
              <Field.Description id="plugin-interval-help">
                {m.plugin_interval_help({ seconds: formatNumber(60) })}
              </Field.Description>
            </Field.Field>
            <Field.Separator />
            <Field.Group
              role="group"
              aria-labelledby="plugin-permissions-title"
              aria-describedby="plugin-permissions-help"
              class="gap-3"
            >
              <h3 id="plugin-permissions-title" class="text-sm font-medium">
                {m.plugin_permissions()}
              </h3>
              <Field.Description id="plugin-permissions-help"
                >{m.plugin_permissions_help()}</Field.Description
              >
              {#if editing.domains.length && !domains.length}
                <Field.Description
                  id="plugin-api-permissions-hint"
                  role="status"
                  class="flex items-start gap-2"
                >
                  <CircleAlertIcon
                    class="mt-0.5 size-4 shrink-0"
                    aria-hidden="true"
                  />
                  <span>{m.plugin_api_permissions_empty()}</span>
                </Field.Description>
              {/if}
              <Field.Group data-slot="checkbox-group">
                {#each editing.domains as domain (domain)}
                  <Field.Field orientation="horizontal" data-disabled={pending}>
                    <Checkbox
                      id={"plugin-domain-" + domain}
                      checked={domains.includes(domain)}
                      disabled={pending}
                      aria-describedby={!domains.length
                        ? "plugin-permissions-help plugin-api-permissions-hint"
                        : "plugin-permissions-help"}
                      onCheckedChange={(checked) => {
                        domains = checked
                          ? [...domains, domain]
                          : domains.filter((value) => value !== domain);
                        consent = false;
                      }}
                    />
                    <Field.Content
                      ><Field.Label
                        for={"plugin-domain-" + domain}
                        class="break-all font-normal"
                        >{m.plugin_allow_api_domain({ domain })}</Field.Label
                      ></Field.Content
                    >
                  </Field.Field>
                {/each}
                <Field.Field
                  orientation="horizontal"
                  data-disabled={pending}
                  data-invalid={saveAttempted && !consent}
                >
                  <Checkbox
                    id="plugin-consent"
                    aria-invalid={saveAttempted && !consent}
                    bind:checked={consent}
                    disabled={pending}
                    aria-describedby={saveAttempted && !consent
                      ? "plugin-permissions-help plugin-consent-error"
                      : "plugin-permissions-help"}
                  />
                  <Field.Content
                    ><Field.Label for="plugin-consent" class="font-normal"
                      >{m.plugin_grant_board()}</Field.Label
                    >{#if saveAttempted && !consent}<Field.Error
                        id="plugin-consent-error"
                        >{m.plugin_consent_required()}</Field.Error
                      >{/if}</Field.Content
                  >
                </Field.Field>
              </Field.Group>
            </Field.Group>
          </Field.Group>
        </ScrollArea>
        <Dialog.Footer class="shrink-0 border-t pt-4">
          <Button type="submit" disabled={pending}>{m.common_save()}</Button
          ><Button variant="outline" disabled={pending} onclick={closeEditor}
            >{m.common_cancel()}</Button
          >
        </Dialog.Footer>
      </form>
    </Dialog.Content>
    <Dialog.Root bind:open={newBoardOpen} busy={creatingBoard}>
      <Dialog.Content class="z-60 sm:max-w-sm">
        <Dialog.Header>
          <Dialog.Title>{m.board_create()}</Dialog.Title>
          <Dialog.Description>{m.board_create_description()}</Dialog.Description
          >
        </Dialog.Header>
        <form
          novalidate
          class="space-y-4"
          onsubmit={(event) => {
            event.preventDefault();
            void createBoard();
          }}
        >
          <Field.Field data-disabled={creatingBoard}>
            <Field.Label for="plugin-new-board-name"
              >{m.board_name()}</Field.Label
            >
            <Input
              id="plugin-new-board-name"
              bind:value={newBoardName}
              maxlength={200}
              required
              disabled={creatingBoard}
            />
            {#if newBoardError}<Field.Error
                >{pluginErrorMessage(newBoardError)}</Field.Error
              >{/if}
          </Field.Field>
          <Dialog.Footer>
            <Button
              variant="outline"
              disabled={creatingBoard}
              onclick={() => (newBoardOpen = false)}>{m.common_cancel()}</Button
            >
            <Button
              type="submit"
              disabled={creatingBoard || !newBoardName.trim()}
              >{#if creatingBoard}<Spinner
                  aria-hidden="true"
                />{/if}{m.common_create()}</Button
            >
          </Dialog.Footer>
        </form>
      </Dialog.Content>
    </Dialog.Root>
    <Dialog.Root
      bind:open={
        () => newColumnField !== null,
        (value) => {
          if (!value) {
            newColumnField = null;
          }
        }
      }
      busy={creatingColumn}
    >
      <Dialog.Content class="z-60 sm:max-w-sm">
        <Dialog.Header
          ><Dialog.Title>{m.column_create()}</Dialog.Title></Dialog.Header
        >
        <form
          novalidate
          class="space-y-4"
          onsubmit={(event) => {
            event.preventDefault();
            void createColumn();
          }}
        >
          <Field.Field data-disabled={creatingColumn}>
            <Field.Label for="plugin-new-column-name"
              >{m.common_name()}</Field.Label
            >
            <Input
              id="plugin-new-column-name"
              bind:value={newColumnName}
              autocomplete="off"
              required
              disabled={creatingColumn}
            />
            {#if newColumnError}<Field.Error
                >{pluginErrorMessage(newColumnError)}</Field.Error
              >{/if}
          </Field.Field>
          <Dialog.Footer>
            <Button
              variant="outline"
              disabled={creatingColumn}
              onclick={() => (newColumnField = null)}
              >{m.common_cancel()}</Button
            >
            <Button
              type="submit"
              disabled={creatingColumn || !newColumnName.trim()}
              >{#if creatingColumn}<Spinner
                  aria-hidden="true"
                />{/if}{m.common_create()}</Button
            >
          </Dialog.Footer>
        </form>
      </Dialog.Content>
    </Dialog.Root>
  </Dialog.Root>
{/if}
