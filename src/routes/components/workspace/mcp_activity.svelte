<script lang="ts">
  import { Button } from "#lib/components/ui/button/index.js";
  import { m } from "#lib/paraglide/messages.js";
  import { formatDateTime } from "#lib/i18n/index.js";
  import { translateCommandError } from "#lib/command-errors.js";
  import { logger } from "#lib/logger.js";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import CircleAlertIcon from "@lucide/svelte/icons/circle-alert";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import { board } from "../../board.svelte";
  import type { McpActivity, McpClient } from "./mcp_server";

  let { clients }: { clients: McpClient[] } = $props();
  let activity = $state<McpActivity[]>([]);

  async function refresh() {
    try {
      activity = await invoke<McpActivity[]>("get_mcp_activity");
    } catch (caught) {
      logger.warn("settings.mcp_activity.failed", caught);
    }
  }

  async function clear() {
    try {
      await invoke("clear_mcp_activity");
      activity = [];
    } catch (caught) {
      logger.warn("settings.mcp_activity_clear.failed", caught);
    }
  }

  onMount(() => {
    if (!isTauri()) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("cardbe:mcp-activity", () => void refresh())
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch((caught) => logger.warn("settings.mcp_activity.failed", caught));
    void refresh();
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  function client_name(id: string) {
    return (
      clients.find((client) => client.id === id)?.name ??
      m.mcp_activity_removed_client()
    );
  }

  function board_name(id: number) {
    return (
      board.boards.find((item) => item.id === id)?.name ??
      m.mcp_activity_unknown_board()
    );
  }
</script>

<div class="space-y-2">
  <div class="flex items-center justify-between gap-2">
    <h3 class="text-sm font-medium">{m.mcp_activity_title()}</h3>
    <Button
      variant="ghost"
      size="sm"
      disabled={activity.length === 0}
      onclick={() => void clear()}>{m.mcp_activity_clear()}</Button
    >
  </div>
  <p class="text-xs text-muted-foreground">{m.mcp_activity_description()}</p>
  {#if activity.length === 0}
    <p class="text-sm text-muted-foreground">{m.mcp_activity_empty()}</p>
  {:else}
    <ul class="divide-y rounded-md border">
      {#each activity as call, index (index)}
        <li class="flex items-start gap-2 px-3 py-2">
          {#if call.error}
            <CircleAlertIcon
              class="mt-0.5 size-4 shrink-0 text-destructive"
              aria-label={m.mcp_activity_failed()}
            />
          {:else}
            <CircleCheckIcon
              class="mt-0.5 size-4 shrink-0 text-muted-foreground"
              aria-label={m.mcp_activity_succeeded()}
            />
          {/if}
          <div class="min-w-0 flex-1 space-y-0.5">
            <div class="flex items-baseline justify-between gap-2">
              <code class="truncate text-xs">{call.tool}</code>
              <time
                class="shrink-0 text-xs text-muted-foreground"
                datetime={new Date(call.at).toISOString()}
                >{formatDateTime(call.at, {
                  dateStyle: "short",
                  timeStyle: "medium",
                })}</time
              >
            </div>
            <p class="truncate text-xs text-muted-foreground">
              {client_name(call.client_id)}
              {#if call.board_id !== null}
                · {board_name(call.board_id)}{/if}
              · {m.mcp_activity_duration({ ms: call.duration_ms })}
            </p>
            {#if call.error}
              <p class="text-xs text-destructive">
                {translateCommandError({ code: call.error })}
              </p>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>
