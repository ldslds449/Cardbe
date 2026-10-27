<script lang="ts">
  import { translateCommandError } from "#lib/command-errors.js";
  import * as m from "#lib/paraglide/messages.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import * as Collapsible from "#lib/components/ui/collapsible/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { Label } from "#lib/components/ui/label/index.js";
  import * as Select from "#lib/components/ui/select/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import { Textarea } from "#lib/components/ui/textarea/index.js";
  import type { IrohNetworkSettingsDraft } from "./iroh_network_settings.svelte";

  let {
    network,
    ondiscard,
    onsave,
  }: {
    network: IrohNetworkSettingsDraft;
    ondiscard: () => void;
    onsave: () => void;
  } = $props();
  const mode_label = $derived(
    network.mode === "automatic"
      ? m.share_network_automatic()
      : network.mode === "direct"
        ? m.share_network_direct()
        : network.mode === "relay"
          ? m.share_network_relay()
          : m.share_network_choose(),
  );
</script>

<ScrollArea
  class="min-h-0 flex-1"
  orientation="vertical"
  type="always"
  scrollbarYClasses="w-2"
>
  <fieldset disabled={network.saving} class="grid min-w-0 gap-4 pr-4">
    <div class="flex items-center justify-between gap-4">
      <Label for="iroh-enabled">{m.share_iroh_enable()}</Label>
      <Switch id="iroh-enabled" bind:checked={network.settings.enabled} />
    </div>
    <Card.Root class="gap-0 p-0 shadow-none">
      <Card.Header class="px-4 py-3">
        <Card.Title class="text-base">{m.share_network_method()}</Card.Title>
      </Card.Header>
      <Card.Content class="grid gap-2 px-4 pb-4">
        <Label for="network-mode" class="sr-only"
          >{m.share_network_mode()}</Label
        >
        <Select.Root
          type="single"
          value={network.mode}
          onValueChange={(value) => network.set_mode(value)}
        >
          <Select.Trigger
            id="network-mode"
            class="w-full"
            aria-describedby="network-mode-help">{mode_label}</Select.Trigger
          >
          <Select.Content>
            <Select.Item value="automatic"
              >{m.share_network_automatic()}</Select.Item
            >
            <Select.Item value="direct">{m.share_network_direct()}</Select.Item>
            <Select.Item value="relay">{m.share_network_relay()}</Select.Item>
          </Select.Content>
        </Select.Root>
        <p id="network-mode-help" class="text-sm text-muted-foreground">
          {#if network.mode === "automatic"}
            {m.share_network_automatic_help()}
          {:else if network.mode === "direct"}
            {m.share_network_direct_help()}
          {:else if network.mode === "relay"}
            {m.share_network_relay_help()}
          {:else}
            {m.share_network_disabled_help()}
          {/if}
        </p>
      </Card.Content>
    </Card.Root>
    <Collapsible.Root
      bind:open={network.advanced_open}
      class="rounded-lg border bg-card"
    >
      <Collapsible.Trigger
        class="group flex w-full items-center gap-2 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span class="flex-1">{m.share_network_advanced()}</span>
        {#if network.customized}<Badge variant="secondary"
            >{m.share_network_customized()}</Badge
          >{/if}
        <ChevronDownIcon
          class="size-4 text-muted-foreground transition-transform group-data-[state=open]:rotate-180"
        />
      </Collapsible.Trigger>
      <Collapsible.Content class="border-t p-4">
        <div class="grid gap-5">
          <div class="grid gap-2">
            <Label for="network-discovery">{m.share_network_addresses()}</Label>
            <Select.Root
              type="single"
              value={network.settings.discovery_enabled
                ? "automatic"
                : "manual"}
              onValueChange={(value) => {
                network.settings.discovery_enabled = value === "automatic";
                network.clear_error("discovery_urls");
              }}
            >
              <Select.Trigger
                id="network-discovery"
                class="w-full"
                aria-describedby="network-discovery-help"
                >{network.settings.discovery_enabled
                  ? m.share_network_find()
                  : m.share_network_manual()}</Select.Trigger
              >
              <Select.Content>
                <Select.Item value="automatic"
                  >{m.share_network_find()}</Select.Item
                >
                <Select.Item value="manual"
                  >{m.share_network_manual()}</Select.Item
                >
              </Select.Content>
            </Select.Root>
            <p
              id="network-discovery-help"
              class="text-sm text-muted-foreground"
            >
              {m.share_network_discovery_help()}
            </p>
          </div>

          <label class="grid gap-2 text-sm font-medium">
            {m.share_network_port()}
            <Input
              id="network-listen-port"
              type="number"
              min={0}
              max={65535}
              step={1}
              bind:value={network.settings.listen_port}
              disabled={!network.settings.direct_ip_enabled}
              aria-describedby={"network-listen-port-help" +
                (network.field_errors.listen_port
                  ? " network-listen-port-error"
                  : "")}
              aria-invalid={Boolean(network.field_errors.listen_port)}
              oninput={() => network.clear_error("listen_port")}
            />
            <span
              id="network-listen-port-help"
              class="font-normal text-muted-foreground"
            >
              {m.share_network_port_help()}
            </span>
            {#if network.field_errors.listen_port}
              <span
                id="network-listen-port-error"
                class="text-sm font-normal text-destructive"
                role="alert"
                >{network.field_errors.listen_port === "port_range"
                  ? m.share_network_port_error()
                  : translateCommandError(
                      network.field_errors.listen_port,
                    )}</span
              >
            {/if}
          </label>
          <label class="grid gap-2 text-sm font-medium">
            {m.share_network_fixed_addresses()}
            <Textarea
              id="network-direct-addresses"
              aria-describedby={"network-direct-addresses-help" +
                (network.field_errors.direct_addresses
                  ? " network-direct-addresses-error"
                  : "")}
              bind:value={network.direct_addresses_text}
              disabled={!network.settings.direct_ip_enabled}
              aria-invalid={Boolean(network.field_errors.direct_addresses)}
              oninput={() => network.clear_error("direct_addresses")}
              rows={3}
              placeholder="192.168.1.20:12345"
            />
            <span
              id="network-direct-addresses-help"
              class="font-normal text-muted-foreground"
              >{m.share_network_fixed_help()}</span
            >
            {#if !network.settings.direct_ip_enabled}<span
                class="text-xs font-normal text-muted-foreground"
                >{m.share_network_enable_direct()}</span
              >{/if}
            {#if network.field_errors.direct_addresses}
              <span
                id="network-direct-addresses-error"
                class="text-sm font-normal text-destructive"
                role="alert"
                >{translateCommandError(
                  network.field_errors.direct_addresses,
                )}</span
              >
            {/if}
          </label>
          <label class="grid gap-2 text-sm font-medium">
            {m.share_network_relay_urls()}
            <Textarea
              id="network-relay-urls"
              aria-describedby={"network-relay-urls-help" +
                (network.field_errors.relay_urls
                  ? " network-relay-urls-error"
                  : "")}
              bind:value={network.relay_urls_text}
              disabled={!network.settings.relay_enabled}
              aria-invalid={Boolean(network.field_errors.relay_urls)}
              oninput={() => network.clear_error("relay_urls")}
              rows={3}
              placeholder="https://relay.example.com"
            />
            <span
              id="network-relay-urls-help"
              class="font-normal text-muted-foreground"
              >{m.share_network_relay_help_urls()}</span
            >
            {#if !network.settings.relay_enabled}<span
                class="text-xs font-normal text-muted-foreground"
                >{m.share_network_enable_relay()}</span
              >{/if}
            {#if network.field_errors.relay_urls}
              <span
                id="network-relay-urls-error"
                class="text-sm font-normal text-destructive"
                role="alert"
                >{translateCommandError(network.field_errors.relay_urls)}</span
              >
            {/if}
          </label>
          <label class="grid gap-2 text-sm font-medium">
            {m.share_network_discovery_urls()}
            <Textarea
              id="network-discovery-urls"
              aria-describedby={"network-discovery-urls-help" +
                (network.field_errors.discovery_urls
                  ? " network-discovery-urls-error"
                  : "")}
              bind:value={network.discovery_urls_text}
              disabled={!network.settings.discovery_enabled}
              aria-invalid={Boolean(network.field_errors.discovery_urls)}
              oninput={() => network.clear_error("discovery_urls")}
              rows={3}
              placeholder="https://discovery.example.com"
            />
            <span
              id="network-discovery-urls-help"
              class="font-normal text-muted-foreground"
              >{m.share_network_discovery_help_urls()}</span
            >
            {#if !network.settings.discovery_enabled}<span
                class="text-xs font-normal text-muted-foreground"
                >{m.share_network_enable_discovery()}</span
              >{/if}
            {#if network.field_errors.discovery_urls}
              <span
                id="network-discovery-urls-error"
                class="text-sm font-normal text-destructive"
                role="alert"
                >{translateCommandError(
                  network.field_errors.discovery_urls,
                )}</span
              >
            {/if}
          </label>
        </div>
      </Collapsible.Content>
    </Collapsible.Root>
  </fieldset>
</ScrollArea>
<div class="shrink-0 space-y-3 border-t pt-3">
  {#if !network.has_transport}
    <p class="text-sm text-destructive" role="alert">
      {m.share_network_enable_transport()}
    </p>
  {/if}
  {#if network.error}
    <p
      class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive"
      role="alert"
    >
      {translateCommandError(network.error)}
    </p>
  {/if}
  <p class="text-xs text-muted-foreground" role="status">
    {network.saving
      ? m.share_network_applying_changes()
      : network.dirty
        ? m.share_network_unapplied()
        : network.restart_failed
          ? m.share_network_retry_activate()
          : network.applied
            ? m.share_network_applied()
            : m.share_network_no_changes()}
  </p>
  <div class="flex flex-wrap items-center justify-between gap-2">
    <Button
      variant="outline"
      disabled={network.saving}
      onclick={() => network.reset()}>{m.share_network_defaults()}</Button
    >
    <div class="flex gap-2">
      <Button
        variant="outline"
        class="min-w-24"
        disabled={network.saving || !network.dirty}
        onclick={ondiscard}>{m.share_network_discard()}</Button
      >
      <Button
        class="min-w-24"
        aria-busy={network.saving}
        onclick={onsave}
        disabled={network.saving ||
          !network.has_transport ||
          (!network.dirty && !network.restart_failed)}
      >
        {network.saving
          ? m.share_network_applying()
          : network.restart_failed && !network.dirty
            ? m.common_retry()
            : m.share_network_apply()}
      </Button>
    </div>
  </div>
</div>
