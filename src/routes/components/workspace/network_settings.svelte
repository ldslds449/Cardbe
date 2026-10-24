<script lang="ts">
  import { onMount } from "svelte";
  import { translateCommandError } from "$lib/command-errors";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { logger } from "$lib/logger";
  import { m } from "$lib/paraglide/messages.js";
  import { registerUnsavedChanges } from "$lib/unsaved-changes";
  import { board } from "../../board.svelte";
  import { IrohNetworkSettingsDraft } from "../share/iroh_network_settings.svelte";
  import IrohNetworkSettingsForm from "../share/iroh_network_settings_form.svelte";
  import { settingsDialog } from "./settings-dialog.svelte";

  let {
    open,
    active,
    dirty = $bindable(false),
    busy = $bindable(false),
  }: {
    open: boolean;
    active: boolean;
    dirty?: boolean;
    busy?: boolean;
  } = $props();
  const network = new IrohNetworkSettingsDraft();
  let loaded = $state(false);
  let load_error = $state<unknown>(null);

  $effect(() => {
    dirty = loaded && !network.loading && network.dirty;
    busy = network.saving;
  });
  $effect(() => {
    if (!open) {
      loaded = false;
      load_error = null;
    } else if (active && !loaded && !network.loading && !load_error) {
      void load();
    }
  });
  onMount(() =>
    registerUnsavedChanges(
      () => open && loaded && !network.loading && network.dirty,
    ),
  );

  async function load() {
    network.loading = true;
    load_error = null;
    try {
      await network.load();
      loaded = true;
    } catch (error) {
      logger.error("iroh.network_settings_load.failed", error);
      load_error = error;
    } finally {
      network.loading = false;
    }
  }

  async function save() {
    await network.save(async (settings, restart_error) => {
      board.iroh_enabled = settings.enabled;
      settingsDialog.networkVersion++;
      if (restart_error === null) {
        board.reconnect_iroh_boards();
      }
    });
  }
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <p class="shrink-0 text-sm text-muted-foreground">
    {m.share_network_description()}
  </p>
  {#if load_error}
    <div class="grid justify-items-start gap-2">
      <p role="alert" class="text-sm text-destructive">
        {translateCommandError(load_error)}
      </p>
      <Button variant="outline" onclick={() => void load()}
        >{m.common_retry()}</Button
      >
    </div>
  {:else if !loaded}
    <div
      class="flex items-center gap-2 py-6 text-sm text-muted-foreground"
      role="status"
    >
      <Spinner aria-hidden="true" />{m.share_loading_settings()}
    </div>
  {:else}
    <IrohNetworkSettingsForm
      {network}
      ondiscard={() => void load()}
      onsave={() => void save()}
    />
  {/if}
</div>
