<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { Checkbox } from "#lib/components/ui/checkbox/index.js";
  import * as Field from "#lib/components/ui/field/index.js";
  import { m } from "#lib/paraglide/messages.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { parseCommandError, type CommandError } from "#lib/command-errors.js";
  import {
    pluginErrorMessage,
    type PluginInstance,
    type PluginState,
  } from "#lib/plugins.js";

  let { review = $bindable(null) }: { review: string | null } = $props();
  let instances = $state<PluginInstance[]>([]);
  let safeMode = $state(false);
  let selected = $state<PluginInstance | null>(null);
  let open = $state(false);
  let busy = $state(false);
  let error = $state<CommandError | null>(null);
  let allowPrivateNetwork = $state(false);
  const shown = new Set<string>();
  const privateNetwork = $derived(!!selected?.pending_domain?.private_network);
  let generation = 0;

  function show(instance: PluginInstance) {
    if (!instance.pending_domain) {
      return;
    }
    shown.add(instance.pending_domain.run_id);
    selected = instance;
    allowPrivateNetwork = false;
    error = null;
    open = true;
  }

  async function refresh() {
    const request = ++generation;
    try {
      const state = await invoke<PluginState>("get_plugin_state");
      if (request !== generation) {
        return;
      }
      safeMode = state.safe_mode;
      instances = state.instances.filter(
        (instance) =>
          instance.enabled && !instance.needs_review && instance.pending_domain,
      );
      if (
        open &&
        selected &&
        !instances.some(
          (instance) =>
            instance.pending_domain?.run_id ===
            selected?.pending_domain?.run_id,
        )
      ) {
        open = false;
        selected = null;
      }
      const manual = instances.find(
        (instance) =>
          instance.pending_domain!.trigger !== "schedule" &&
          !shown.has(instance.pending_domain!.run_id),
      );
      if (!open && manual && !safeMode) {
        show(manual);
      }
    } catch (cause) {
      if (request === generation) {
        error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
      }
    }
  }

  $effect(() => {
    const instance = instances.find((instance) => instance.id === review);
    if (instance && !busy) {
      show(instance);
      review = null;
    }
  });

  onMount(() => {
    if (!isTauri()) {
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("cardbe:plugins-changed", () => void refresh())
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unlisten = stop;
          void refresh();
        }
      })
      .catch((cause) => {
        if (!disposed) {
          error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
        }
      });
    const focus = () => void refresh();
    window.addEventListener("focus", focus);
    return () => {
      disposed = true;
      generation++;
      unlisten?.();
      window.removeEventListener("focus", focus);
    };
  });

  async function resolve(allow: boolean) {
    const instance = selected;
    if (busy || !instance?.pending_domain) {
      return;
    }
    busy = true;
    error = null;
    try {
      await invoke("resolve_plugin_domain", {
        instanceId: instance.id,
        runId: instance.pending_domain.run_id,
        allow,
        privateNetwork: allow && allowPrivateNetwork,
      });
      open = false;
      selected = null;
      await refresh();
    } catch (cause) {
      error = parseCommandError(cause) ?? { code: "INTERNAL_ERROR" };
    } finally {
      busy = false;
    }
  }
</script>

{#if instances.length}
  <Button variant="outline" size="sm" onclick={() => show(instances[0])}>
    <ShieldCheckIcon class="size-4" aria-hidden="true" />
    {m.plugin_pending_permissions({ count: formatNumber(instances.length) })}
  </Button>
{/if}

<Dialog.Root bind:open {busy}>
  <Dialog.Content class="z-80" showCloseButton={!busy}>
    <Dialog.Header>
      <Dialog.Title
        >{privateNetwork
          ? m.plugin_private_network_request_title()
          : m.plugin_domain_request_title()}</Dialog.Title
      >
      <Dialog.Description>
        {(privateNetwork
          ? m.plugin_private_network_request_description
          : m.plugin_domain_request_description)({
          name: selected?.name ?? "",
          domain: selected?.pending_domain?.domain ?? "",
        })}
      </Dialog.Description>
    </Dialog.Header>
    <p class="break-all rounded-md border bg-muted p-3 font-mono text-sm">
      {selected?.pending_domain?.domain}
    </p>
    <p class="text-sm text-muted-foreground">
      {privateNetwork
        ? m.plugin_private_network_request_help({
            domain: selected?.pending_domain?.domain ?? "",
          })
        : m.plugin_domain_request_help()}
    </p>
    {#if !privateNetwork}
      <Field.Field orientation="horizontal" data-disabled={busy}>
        <Checkbox
          id="plugin-domain-private-network"
          bind:checked={allowPrivateNetwork}
          disabled={busy}
          aria-describedby="plugin-domain-private-network-help"
        />
        <Field.Content>
          <Field.Label for="plugin-domain-private-network" class="font-normal"
            >{m.plugin_domain_request_private_network()}</Field.Label
          >
          <Field.Description id="plugin-domain-private-network-help"
            >{m.plugin_domain_request_private_network_help()}</Field.Description
          >
        </Field.Content>
      </Field.Field>
    {/if}
    {#if error}<p role="alert" class="text-sm text-destructive">
        {pluginErrorMessage(error)}
      </p>{/if}
    <Dialog.Footer>
      <Button
        variant="outline"
        disabled={busy}
        onclick={() => void resolve(false)}>{m.plugin_domain_deny()}</Button
      >
      <Button disabled={busy || safeMode} onclick={() => void resolve(true)}
        >{m.plugin_domain_allow()}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
