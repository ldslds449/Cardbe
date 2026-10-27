<script lang="ts">
  import * as AlertDialog from "#lib/components/ui/alert-dialog/index.js";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { Checkbox } from "#lib/components/ui/checkbox/index.js";
  import * as Collapsible from "#lib/components/ui/collapsible/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Spinner } from "#lib/components/ui/spinner/index.js";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import { m } from "#lib/paraglide/messages.js";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "#lib/command-errors.js";
  import { logger } from "#lib/logger.js";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { toast } from "svelte-sonner";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import PlayIcon from "@lucide/svelte/icons/play";
  import PlugIcon from "@lucide/svelte/icons/plug";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import { board } from "../../board.svelte";
  import McpActivity from "./mcp_activity.svelte";
  import {
    client_configuration,
    valid_port,
    type McpClient,
    type McpClientToken,
    type McpServer,
  } from "./mcp_server";

  let server = $state<McpServer | null>(null);
  let error = $state<CommandError | null>(null);
  let saving = $state(false);
  let port_text = $state("");
  let editor = $state<{
    id: string | null;
    name: string;
    all_boards: boolean;
    boards: number[];
    write: boolean;
  } | null>(null);
  /** Connection details for one client; `token` is null when it can't be shown. */
  let revealed = $state<{
    client_id: string;
    name: string;
    token: string | null;
    saved: boolean;
  } | null>(null);
  let confirming = $state<{
    kind: "delete" | "regenerate";
    client: McpClient;
  } | null>(null);

  const header = $derived(
    revealed?.token ? `Authorization: Bearer ${revealed.token}` : "",
  );
  const configuration = $derived(
    revealed?.token && server
      ? client_configuration(server.endpoint, revealed.token)
      : "",
  );

  const port = $derived(Number(port_text));
  const port_changed = $derived(
    server !== null && port_text.trim() !== String(server.port),
  );

  function show(next: McpServer) {
    server = next;
    port_text = String(next.port);
  }

  async function load() {
    try {
      show(await invoke<McpServer>("get_mcp_server"));
    } catch (caught) {
      logger.warn("settings.mcp_load.failed", caught);
      error = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
    }
  }

  onMount(() => void load());

  // Saving keeps controls enabled so focus and the dialog scroll position stay put.
  async function run<T>(
    action: () => Promise<T>,
    reload_on_error = false,
  ): Promise<T | undefined> {
    if (saving) {
      return;
    }
    saving = true;
    error = null;
    try {
      return await action();
    } catch (caught) {
      logger.warn("settings.mcp_save.failed", caught);
      error = parseCommandError(caught) ?? { code: "INTERNAL_ERROR" };
      // Settings may have been saved even when the server could not start.
      if (reload_on_error) {
        await load();
      }
    } finally {
      saving = false;
    }
  }

  function save_server(changes: Partial<McpServer>) {
    if (!server) {
      return;
    }
    const next = { ...server, ...changes };
    void run(async () => {
      show(
        await invoke<McpServer>("set_mcp_server", {
          enabled: next.enabled,
          autoStart: next.auto_start,
          port: next.port,
        }),
      );
    }, true);
  }

  function start() {
    void run(
      async () => show(await invoke<McpServer>("start_mcp_server")),
      true,
    );
  }

  function status_label(status: McpServer["status"]) {
    switch (status) {
      case "running":
        return m.mcp_status_running();
      case "failed":
        return m.mcp_status_failed();
      case "stopped":
        return m.mcp_status_stopped();
    }
  }

  function access_label(client: McpClient) {
    return m.mcp_client_access({
      access: client.write
        ? m.mcp_client_access_write()
        : m.mcp_client_access_read(),
      boards: client.all_boards
        ? m.mcp_client_all_boards_label()
        : m.mcp_client_board_count({
            count: client.boards.filter((id) =>
              board.boards.some((item) => item.id === id),
            ).length,
          }),
    });
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(m.mcp_copied());
    } catch (caught) {
      logger.warn("settings.mcp_copy.failed", caught);
      toast.error(m.mcp_copy_failed());
    }
  }

  function edit(client: McpClient | null) {
    revealed = null;
    editor = client
      ? {
          id: client.id,
          name: client.name,
          all_boards: client.all_boards,
          boards: client.boards.filter((id) =>
            board.boards.some((item) => item.id === id),
          ),
          write: client.write,
        }
      : {
          id: null,
          name: "",
          all_boards: true,
          boards: board.active_board_id ? [board.active_board_id] : [],
          write: false,
        };
  }

  function toggle_board(id: number, checked: boolean) {
    if (!editor) {
      return;
    }
    editor.boards = checked
      ? [...editor.boards, id]
      : editor.boards.filter((board_id) => board_id !== id);
  }

  function can_save(draft: NonNullable<typeof editor>) {
    return (
      draft.name.trim() !== "" && (draft.all_boards || draft.boards.length > 0)
    );
  }

  function save_client() {
    const draft = editor;
    if (!draft || !can_save(draft)) {
      return;
    }
    void run(async () => {
      if (draft.id === null) {
        const created = await invoke<McpClientToken>("create_mcp_client", {
          name: draft.name,
          allBoards: draft.all_boards,
          boards: draft.boards,
          write: draft.write,
        });
        show(created.server);
        editor = null;
        revealed = {
          client_id: created.client_id,
          name: draft.name.trim(),
          token: created.token,
          saved: created.token_saved,
        };
      } else {
        show(
          await invoke<McpServer>("update_mcp_client", {
            clientId: draft.id,
            name: draft.name,
            allBoards: draft.all_boards,
            boards: draft.boards,
            write: draft.write,
          }),
        );
        editor = null;
      }
    });
  }

  function connect(client: McpClient) {
    void run(async () => {
      const token = await invoke<string | null>("get_mcp_client_token", {
        clientId: client.id,
      });
      editor = null;
      revealed = {
        client_id: client.id,
        name: client.name,
        token,
        saved: token !== null,
      };
    });
  }

  function confirm() {
    const target = confirming;
    if (!target) {
      return;
    }
    void run(async () => {
      if (target.kind === "delete") {
        show(
          await invoke<McpServer>("delete_mcp_client", {
            clientId: target.client.id,
          }),
        );
        if (editor?.id === target.client.id) {
          editor = null;
        }
      } else {
        const regenerated = await invoke<McpClientToken>(
          "regenerate_mcp_client_token",
          { clientId: target.client.id },
        );
        show(regenerated.server);
        editor = null;
        revealed = {
          client_id: target.client.id,
          name: target.client.name,
          token: regenerated.token,
          saved: regenerated.token_saved,
        };
      }
      confirming = null;
    });
  }
</script>

{#snippet copy_field(id: string, label: string, value: string)}
  <div class="grid gap-1.5">
    <label for={id} class="text-xs font-medium">{label}</label>
    <div class="flex gap-2">
      <Input {id} class="h-8 font-mono text-xs" readonly {value} />
      <Button
        variant="outline"
        size="icon-sm"
        class="size-8 shrink-0"
        aria-label={m.mcp_copy_value({ name: label })}
        title={m.mcp_copy_value({ name: label })}
        onclick={() => void copy(value)}
      >
        <CopyIcon />
      </Button>
    </div>
  </div>
{/snippet}

<fieldset class="relative space-y-4" aria-busy={saving}>
  <legend class="flex items-center gap-2 text-sm font-medium">
    {m.mcp_title()}
    {#if server?.enabled}
      <Badge
        variant={server.status === "running"
          ? "default"
          : server.status === "failed"
            ? "destructive"
            : "secondary"}
        aria-live="polite">{status_label(server.status)}</Badge
      >
    {/if}
  </legend>
  <Switch
    class="absolute end-0 top-0"
    checked={server?.enabled ?? false}
    disabled={server === null}
    aria-label={m.mcp_enable()}
    aria-describedby="mcp-description"
    onCheckedChange={(enabled) => save_server({ enabled })}
  />
  <p id="mcp-description" class="pe-12 text-sm text-muted-foreground">
    {m.mcp_description()}
  </p>
  {#if error}
    <p role="alert" class="text-xs text-destructive">
      {translateCommandError(error)}
    </p>
  {/if}
  {#if server?.enabled}
    {#if server.status !== "running"}
      <Button size="sm" onclick={start}><PlayIcon />{m.mcp_start()}</Button>
    {/if}
    {@render copy_field("mcp-endpoint", m.mcp_endpoint(), server.endpoint)}
    <Collapsible.Root>
      <Collapsible.Trigger>
        {#snippet child({ props })}
          <Button {...props} variant="ghost" size="sm" class="group -ms-2.5">
            <ChevronDownIcon
              class="-rotate-90 transition-transform group-data-[state=open]:rotate-0"
            />{m.mcp_advanced()}
          </Button>
        {/snippet}
      </Collapsible.Trigger>
      <Collapsible.Content class="grid gap-3 ps-5 pt-3">
        <div class="grid gap-1.5">
          <label for="mcp-port" class="text-xs font-medium"
            >{m.mcp_port()}</label
          >
          <form
            class="flex gap-2"
            onsubmit={(event) => {
              event.preventDefault();
              if (port_changed && valid_port(port)) {
                save_server({ port });
              }
            }}
          >
            <Input
              id="mcp-port"
              class="h-8 w-28"
              inputmode="numeric"
              aria-describedby="mcp-port-hint"
              aria-invalid={!valid_port(port)}
              bind:value={port_text}
            />
            <Button
              type="submit"
              variant="outline"
              size="sm"
              class="h-8"
              disabled={!port_changed || !valid_port(port)}
              >{m.mcp_port_apply()}</Button
            >
          </form>
          <p id="mcp-port-hint" class="text-xs text-muted-foreground">
            {m.mcp_port_hint()}
          </p>
        </div>
        <div class="flex items-center gap-2">
          <Checkbox
            id="mcp-auto-start"
            checked={server.auto_start}
            onCheckedChange={(checked) =>
              save_server({ auto_start: checked === true })}
          />
          <label for="mcp-auto-start" class="text-sm"
            >{m.mcp_auto_start()}</label
          >
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
  {/if}

  {#if server}
    <div class="space-y-2">
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-medium">{m.mcp_clients_title()}</h3>
        <Button variant="outline" size="sm" onclick={() => edit(null)}
          ><PlusIcon />{m.mcp_client_add()}</Button
        >
      </div>
      {#if server.clients.length === 0}
        <p
          class="rounded-md border border-dashed px-3 py-4 text-center text-sm text-muted-foreground"
        >
          {m.mcp_clients_empty()}
        </p>
      {:else}
        <ul class="divide-y rounded-md border">
          {#each server.clients as client (client.id)}
            {@render client_row(client)}
          {/each}
        </ul>
      {/if}
    </div>
    <McpActivity clients={server.clients} />
  {/if}
</fieldset>

{#snippet client_row(client: McpClient)}
  <li class="flex items-center gap-2 px-3 py-2">
    <div class="min-w-0 flex-1">
      <p class="truncate text-sm font-medium" title={client.name}>
        {client.name}
      </p>
      <p class="truncate text-xs text-muted-foreground">
        {access_label(client)}
      </p>
    </div>
    <Button
      variant="outline"
      size="sm"
      class="shrink-0"
      onclick={() => connect(client)}
      ><PlugIcon />{m.mcp_client_connect()}</Button
    >
    <DropdownMenu.Root>
      <DropdownMenu.Trigger>
        {#snippet child({ props })}
          <Button
            {...props}
            id="mcp-client-options-{client.id}"
            variant="ghost"
            size="icon-sm"
            class="size-7 text-muted-foreground"
            aria-label={m.mcp_client_options_for({ name: client.name })}
            title={m.mcp_client_options_for({ name: client.name })}
          >
            <EllipsisIcon />
          </Button>
        {/snippet}
      </DropdownMenu.Trigger>
      <DropdownMenu.Content
        align="end"
        class="min-w-44"
        onCloseAutoFocus={(event) => {
          // Keep focus in the settings dialog; see global shortcut settings.
          event.preventDefault();
          const trigger = document.getElementById(
            `mcp-client-options-${client.id}`,
          );
          queueMicrotask(() => trigger?.focus({ preventScroll: true }));
        }}
      >
        <DropdownMenu.Item onclick={() => edit(client)}>
          <PencilIcon />{m.common_edit()}
        </DropdownMenu.Item>
        <DropdownMenu.Item
          onclick={() => (confirming = { kind: "regenerate", client })}
        >
          <RefreshCwIcon />{m.mcp_token_regenerate()}
        </DropdownMenu.Item>
        <DropdownMenu.Item
          variant="destructive"
          onclick={() => (confirming = { kind: "delete", client })}
        >
          <Trash2Icon />{m.mcp_client_remove()}
        </DropdownMenu.Item>
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </li>
{/snippet}

<Dialog.Root
  bind:open={
    () => editor !== null || revealed !== null,
    (value) => {
      if (!value) {
        editor = null;
        revealed = null;
      }
    }
  }
  draft={editor ?? undefined}
  busy={saving}
>
  <Dialog.Content class="sm:max-w-md">
    {#if editor}
      <Dialog.Header>
        <Dialog.Title
          >{editor.id === null
            ? m.mcp_client_add()
            : m.mcp_client_edit()}</Dialog.Title
        >
        <Dialog.Description
          >{m.mcp_client_edit_description()}</Dialog.Description
        >
      </Dialog.Header>
      <form
        class="grid min-w-0 gap-4"
        onsubmit={(event) => {
          event.preventDefault();
          save_client();
        }}
      >
        <div class="grid gap-1.5">
          <label for="mcp-client-name" class="text-xs font-medium"
            >{m.common_name()}</label
          >
          <Input
            id="mcp-client-name"
            class="h-8"
            maxlength={100}
            placeholder={m.mcp_client_name_placeholder()}
            bind:value={editor.name}
          />
        </div>
        <fieldset class="grid min-w-0 gap-1.5">
          <legend class="mb-1.5 text-xs font-medium"
            >{m.mcp_client_boards()}</legend
          >
          <div class="flex items-center gap-2">
            <Checkbox
              id="mcp-client-all-boards"
              checked={editor.all_boards}
              onCheckedChange={(checked) => {
                if (editor) {
                  editor.all_boards = checked === true;
                }
              }}
            />
            <label for="mcp-client-all-boards" class="text-sm"
              >{m.mcp_client_all_boards()}</label
            >
          </div>
          {#if !editor.all_boards}
            <ScrollArea
              class="[&>[data-slot=scroll-area-viewport]]:max-h-48"
              orientation="vertical"
            >
              <div class="grid gap-1.5 ps-6 pe-3">
                {#each board.boards as item (item.id)}
                  <div class="flex min-w-0 items-center gap-2">
                    <Checkbox
                      id="mcp-client-board-{item.id}"
                      checked={editor.boards.includes(item.id)}
                      onCheckedChange={(checked) =>
                        toggle_board(item.id, checked === true)}
                    />
                    <label
                      for="mcp-client-board-{item.id}"
                      class="min-w-0 truncate text-sm"
                      title={item.name}>{item.name}</label
                    >
                  </div>
                {/each}
              </div>
            </ScrollArea>
          {/if}
        </fieldset>
        <div class="flex items-start gap-2">
          <Checkbox
            id="mcp-client-write"
            class="mt-0.5"
            checked={editor.write}
            onCheckedChange={(checked) => {
              if (editor) {
                editor.write = checked === true;
              }
            }}
          />
          <label for="mcp-client-write" class="text-sm"
            >{m.mcp_client_write()}</label
          >
        </div>
        <Dialog.Footer>
          <Button
            type="button"
            variant="outline"
            disabled={saving}
            onclick={() => (editor = null)}>{m.common_cancel()}</Button
          >
          <Button type="submit" disabled={saving || !can_save(editor)}
            >{editor.id === null ? m.common_add() : m.common_save()}</Button
          >
        </Dialog.Footer>
      </form>
    {:else if revealed}
      <Dialog.Header>
        <Dialog.Title
          >{m.mcp_connect_title({ name: revealed.name })}</Dialog.Title
        >
        <Dialog.Description>{m.mcp_connect_description()}</Dialog.Description>
      </Dialog.Header>
      {#if revealed.token !== null && server}
        <div class="grid min-w-0 gap-4">
          {@render copy_field(
            "mcp-connect-endpoint",
            m.mcp_endpoint(),
            server.endpoint,
          )}
          {@render copy_field("mcp-connect-header", m.mcp_header(), header)}
          <div class="grid min-w-0 gap-1.5">
            <span class="text-xs font-medium">{m.mcp_configuration()}</span>
            <div class="relative min-w-0 rounded-md border bg-muted/40">
              <ScrollArea
                class="[&>[data-slot=scroll-area-viewport]]:max-h-48"
                orientation="both"
              >
                <pre class="p-3 pe-12 font-mono text-xs"><code
                    >{configuration}</code
                  ></pre>
              </ScrollArea>
              <Button
                variant="outline"
                size="icon-sm"
                class="absolute end-2 top-2 size-7"
                aria-label={m.mcp_copy_value({ name: m.mcp_configuration() })}
                title={m.mcp_copy_value({ name: m.mcp_configuration() })}
                onclick={() => void copy(configuration)}
              >
                <CopyIcon />
              </Button>
            </div>
          </div>
          <p
            class={[
              "text-xs",
              revealed.saved ? "text-muted-foreground" : "text-warning",
            ]}
          >
            {revealed.saved
              ? m.mcp_token_saved_hint()
              : m.mcp_token_once_hint()}
          </p>
        </div>
      {:else}
        <p class="text-sm text-muted-foreground">
          {m.mcp_token_unavailable()}
        </p>
      {/if}
      <Dialog.Footer>
        {#if revealed.token === null}
          <Button
            variant="outline"
            onclick={() => {
              const client = server?.clients.find(
                (item) => item.id === revealed?.client_id,
              );
              if (client) {
                confirming = { kind: "regenerate", client };
              }
            }}>{m.mcp_token_regenerate()}</Button
          >
        {/if}
        <Button onclick={() => (revealed = null)}>{m.common_done()}</Button>
      </Dialog.Footer>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<AlertDialog.Root
  open={confirming !== null}
  onOpenChange={(value) => {
    if (!value && !saving) {
      confirming = null;
    }
  }}
>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title
        >{confirming?.kind === "delete"
          ? m.mcp_client_remove()
          : m.mcp_token_regenerate()}</AlertDialog.Title
      >
      <AlertDialog.Description
        >{confirming?.kind === "delete"
          ? m.mcp_client_remove_confirm({ name: confirming.client.name })
          : m.mcp_token_regenerate_confirm({
              name: confirming?.client.name ?? "",
            })}</AlertDialog.Description
      >
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={saving}
        >{m.common_cancel()}</AlertDialog.Cancel
      >
      <Button
        variant={confirming?.kind === "delete" ? "destructive" : "default"}
        disabled={saving}
        onclick={confirm}
      >
        {#if saving}<Spinner aria-hidden="true" />{/if}
        {confirming?.kind === "delete"
          ? m.mcp_client_remove()
          : m.mcp_token_regenerate()}
      </Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
