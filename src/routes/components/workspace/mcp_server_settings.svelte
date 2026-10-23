<script lang="ts">
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { m } from "$lib/paraglide/messages";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "$lib/command-errors";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { toast } from "svelte-sonner";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import EllipsisIcon from "@lucide/svelte/icons/ellipsis";
  import PencilIcon from "@lucide/svelte/icons/pencil";
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
  let revealed = $state<{
    name: string;
    token: string;
    saved: boolean;
  } | null>(null);
  let confirming = $state<{
    kind: "delete" | "regenerate";
    client: McpClient;
  } | null>(null);

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
        revealed = {
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
      }
      editor = null;
    });
  }

  function copy_token(client: McpClient) {
    void run(async () => {
      const token = await invoke<string | null>("get_mcp_client_token", {
        clientId: client.id,
      });
      if (token === null) {
        toast.error(m.mcp_token_unavailable());
        return;
      }
      await copy(token);
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
          name: target.client.name,
          token: regenerated.token,
          saved: regenerated.token_saved,
        };
      }
      confirming = null;
    });
  }
</script>

<fieldset class="relative space-y-3" aria-busy={saving}>
  <legend class="text-sm font-medium">{m.mcp_title()}</legend>
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
    <div class="flex items-center gap-2" aria-live="polite">
      <Badge
        variant={server.status === "running"
          ? "default"
          : server.status === "failed"
            ? "destructive"
            : "secondary"}>{status_label(server.status)}</Badge
      >
      {#if server.status !== "running"}
        <Button variant="outline" size="sm" class="h-7" onclick={start}
          >{m.mcp_start()}</Button
        >
      {/if}
    </div>
    <div class="grid gap-1.5">
      <label for="mcp-endpoint" class="text-xs font-medium"
        >{m.mcp_endpoint()}</label
      >
      <div class="flex gap-2">
        <Input
          id="mcp-endpoint"
          class="h-8 font-mono text-xs"
          readonly
          value={server.endpoint}
        />
        <Button
          variant="outline"
          size="icon-sm"
          class="size-8 shrink-0"
          aria-label={m.mcp_copy_endpoint()}
          title={m.mcp_copy_endpoint()}
          onclick={() => server && void copy(server.endpoint)}
        >
          <CopyIcon />
        </Button>
      </div>
    </div>
    <div class="grid gap-1.5">
      <label for="mcp-port" class="text-xs font-medium">{m.mcp_port()}</label>
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
      <label for="mcp-auto-start" class="text-sm">{m.mcp_auto_start()}</label>
    </div>
  {/if}

  {#if server}
    <div class="space-y-2">
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-medium">{m.mcp_clients_title()}</h3>
        {#if editor?.id !== null}
          <Button
            variant="ghost"
            size="sm"
            class="h-7"
            onclick={() => edit(null)}><PlusIcon />{m.mcp_client_add()}</Button
          >
        {/if}
      </div>
      {#if server.clients.length === 0 && !editor}
        <p class="text-sm text-muted-foreground">{m.mcp_clients_empty()}</p>
      {/if}
      <ul class="-mx-2">
        {#each server.clients as client (client.id)}
          <li
            class="group flex items-center gap-2 rounded-md px-2 py-1 hover:bg-muted/50"
          >
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm" title={client.name}>{client.name}</p>
              <p class="truncate text-xs text-muted-foreground">
                {access_label(client)}
              </p>
            </div>
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
                <DropdownMenu.Item onclick={() => copy_token(client)}>
                  <CopyIcon />{m.mcp_token_copy()}
                </DropdownMenu.Item>
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
        {/each}
      </ul>

      {#if editor}
        <form
          class="grid gap-3 rounded-md border p-3"
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
          <fieldset class="grid gap-1.5">
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
            {#each editor.all_boards ? [] : board.boards as item (item.id)}
              <div class="flex items-center gap-2 ps-6">
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
          <div class="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onclick={() => (editor = null)}
              >{m.common_cancel()}</Button
            >
            <Button type="submit" size="sm" disabled={!can_save(editor)}
              >{editor.id === null ? m.common_add() : m.common_save()}</Button
            >
          </div>
        </form>
      {/if}

      {#if revealed}
        <div class="grid gap-2 rounded-md border p-3" role="status">
          <p class="text-sm font-medium">
            {m.mcp_token_title({ name: revealed.name })}
          </p>
          <div class="flex gap-2">
            <Input
              class="h-8 font-mono text-xs"
              readonly
              aria-label={m.mcp_token_title({ name: revealed.name })}
              value={revealed.token}
            />
            <Button
              variant="outline"
              size="icon-sm"
              class="size-8 shrink-0"
              aria-label={m.mcp_token_copy()}
              title={m.mcp_token_copy()}
              onclick={() => revealed && void copy(revealed.token)}
            >
              <CopyIcon />
            </Button>
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
          <div class="flex flex-wrap justify-end gap-2">
            <Button
              variant="outline"
              size="sm"
              onclick={() =>
                revealed &&
                server &&
                void copy(
                  client_configuration(server.endpoint, revealed.token),
                )}>{m.mcp_copy_configuration()}</Button
            >
            <Button size="sm" onclick={() => (revealed = null)}
              >{m.common_done()}</Button
            >
          </div>
        </div>
      {/if}
    </div>
    <McpActivity clients={server.clients} />
  {/if}
</fieldset>

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
