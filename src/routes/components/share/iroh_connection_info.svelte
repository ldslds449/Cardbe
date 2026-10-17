<script lang="ts">
  import { formatNumber, getLocale } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import * as Collapsible from "$lib/components/ui/collapsible/index.js";
  import type { IrohConnectionInfo } from "../../type/iroh-share";

  let {
    info,
    waiting_for_approval = false,
  }: {
    info: IrohConnectionInfo | null | undefined;
    waiting_for_approval?: boolean;
  } = $props();
  const sync_time = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      dateStyle: "medium",
      timeStyle: "short",
    }),
  );
</script>

<Collapsible.Root class="rounded-md border bg-background">
  <Collapsible.Trigger
    class="group flex w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-xs font-medium hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
  >
    <span>{m.share_connection_last()}</span>
    <span class="flex items-center gap-2">
      {#if info && info.paths.length > 0}
        <Badge variant="outline"
          >{[
            ...new Set(
              info.paths.map((path) =>
                path.kind === "Direct"
                  ? m.share_direct_ip()
                  : path.kind === "Relay"
                    ? m.share_relay_servers()
                    : m.share_custom_transport(),
              ),
            ),
          ].join(" + ")}</Badge
        >
      {/if}
      <ChevronDownIcon
        class="size-3.5 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
      />
    </span>
  </Collapsible.Trigger>
  <Collapsible.Content class="space-y-2 border-t px-3 py-3 text-xs">
    {#if info}
      <p class="text-muted-foreground">
        {m.share_connection_observed({
          time: sync_time.format(new Date(info.observed_at)),
        })}{#if waiting_for_approval}
          {m.share_approval_reconnect()}{/if}
      </p>
      {#if info.paths.length > 0}
        <Collapsible.Root>
          <Collapsible.Trigger
            class="group flex items-center gap-2 rounded-md px-2 py-1.5 text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {m.share_connection_paths({
              count: formatNumber(info.paths.length),
            })}
            <ChevronDownIcon
              class="size-3.5 transition-transform group-data-[state=open]:rotate-180"
            />
          </Collapsible.Trigger>
          <Collapsible.Content class="space-y-3 pt-2">
            {#each [...new Set(info.paths.map((path) => path.kind))] as kind}
              <div class="space-y-1">
                <p class="font-medium">
                  {kind === "Direct"
                    ? m.share_direct_addresses()
                    : kind === "Relay"
                      ? m.share_relay_servers()
                      : m.share_custom_transport()}
                </p>
                <code
                  class="block whitespace-pre-wrap break-all rounded-md bg-muted px-2 py-1 select-all"
                  >{[
                    ...new Set(
                      info.paths
                        .filter((path) => path.kind === kind)
                        .map((path) => path.address),
                    ),
                  ].join("\n")}</code
                >
              </div>
            {/each}
            <p class="text-muted-foreground">
              {m.share_connection_paths_help()}
            </p>
          </Collapsible.Content>
        </Collapsible.Root>
      {:else}
        <p class="text-muted-foreground">
          {m.share_connection_path_unknown()}
        </p>
      {/if}
    {:else}
      <p class="text-muted-foreground">
        {m.share_connection_none()}
      </p>
    {/if}
  </Collapsible.Content>
</Collapsible.Root>
