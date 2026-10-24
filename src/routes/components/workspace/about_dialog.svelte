<script lang="ts">
  import { translateCommandError } from "$lib/command-errors";
  import * as m from "$lib/paraglide/messages.js";
  import { invoke } from "@tauri-apps/api/core";
  import ExternalLinkIcon from "@lucide/svelte/icons/external-link";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import GithubIcon from "@lucide/svelte/icons/github";
  import InfoIcon from "@lucide/svelte/icons/info";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import { toast } from "svelte-sonner";

  import * as Card from "$lib/components/ui/card/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { logger } from "$lib/logger";
  import updateConfig from "../../../config/update.json";
  import type { DiagnosticInfo } from "./diagnostics";

  let {
    open = $bindable(false),
    update_check_in_progress = false,
    onCheckForUpdates,
  }: {
    open?: boolean;
    update_check_in_progress?: boolean;
    onCheckForUpdates: (manual?: boolean) => void | Promise<void>;
  } = $props();

  let diagnostics = $state<DiagnosticInfo | null>(null);
  let diagnostics_loading = $state(false);
  let was_loaded = false;

  const github_url = `https://github.com/${updateConfig.githubOwner}/${updateConfig.githubRepository}`;
  const license_url = `${github_url}/blob/main/LICENSE`;

  $effect(() => {
    if (open && !was_loaded) {
      was_loaded = true;
      void load_diagnostics();
    } else if (!open) {
      was_loaded = false;
    }
  });

  async function load_diagnostics() {
    diagnostics_loading = true;
    try {
      diagnostics = await invoke<DiagnosticInfo>("get_diagnostics");
    } catch (error) {
      logger.error("diagnostics.load.failed", error);
    } finally {
      diagnostics_loading = false;
    }
  }

  async function open_external_url(url: string, event: string) {
    try {
      await invoke("open_external_url", { url });
    } catch (error) {
      logger.error(event, error);
      toast.error(translateCommandError(error));
    }
  }

  const close_button_class =
    "inline-flex h-9 items-center justify-center rounded-md border border-border bg-background px-3 text-sm font-medium shadow-xs transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/50";
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-[500px]"
  >
    <Dialog.Header class="border-b pb-4">
      <div class="flex items-start gap-3 pr-8">
        <div
          class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary"
        >
          <InfoIcon class="size-5" />
        </div>
        <div class="space-y-1">
          <Dialog.Title>{m.menu_about()}</Dialog.Title>
          <Dialog.Description>
            {m.ui_task_management_for_focused_work()}
          </Dialog.Description>
        </div>
      </div>
    </Dialog.Header>

    <Card.Root class="gap-0 overflow-hidden py-0">
      <Card.Header class="bg-muted/30 py-4">
        <Card.Title class="text-base">Cardbe</Card.Title>
        <Card.Description
          >{m.ui_desktop_task_management_kept_simple()}</Card.Description
        >
      </Card.Header>
      <Card.Content class="grid grid-cols-2 gap-4 py-4">
        <div class="space-y-1">
          <div class="text-xs font-medium text-muted-foreground">
            {m.ui_version()}
          </div>
          <div class="font-mono text-sm">
            {diagnostics?.appVersion ??
              (diagnostics_loading ? m.ui_loading() : "Unknown")}
          </div>
        </div>
        <div class="space-y-1">
          <div class="text-xs font-medium text-muted-foreground">
            {m.ui_build()}
          </div>
          <div class="font-mono text-sm">
            {diagnostics?.buildCommit ??
              (diagnostics_loading ? m.ui_loading() : "Unknown")}
          </div>
        </div>
      </Card.Content>
    </Card.Root>

    <div class="grid gap-2">
      <Button
        class="w-full justify-between"
        disabled={update_check_in_progress}
        onclick={() => void onCheckForUpdates(true)}
      >
        <span class="flex items-center gap-2">
          {#if update_check_in_progress}
            <LoaderCircleIcon class="size-4 animate-spin" />
          {:else}
            <RefreshCwIcon class="size-4" />
          {/if}
          {m.ui_check_for_updates()}
        </span>
        <span class="text-xs font-normal opacity-70"
          >{m.ui_current_version()}</span
        >
      </Button>
      <Button
        variant="outline"
        class="w-full justify-between"
        onclick={() => void open_external_url(github_url, "help.github.failed")}
      >
        <span class="flex items-center gap-2">
          <GithubIcon class="size-4" />
          {m.ui_github_repository()}
        </span>
        <ExternalLinkIcon class="size-4 text-muted-foreground" />
      </Button>
      <Button
        variant="outline"
        class="w-full justify-between"
        onclick={() =>
          void open_external_url(license_url, "help.license.failed")}
      >
        <span class="flex items-center gap-2">
          <FileTextIcon class="size-4" />
          {m.ui_licenses()}
        </span>
        <ExternalLinkIcon class="size-4 text-muted-foreground" />
      </Button>
    </div>

    <Dialog.Footer class="border-t pt-4">
      <Dialog.Close class={close_button_class}>{m.common_close()}</Dialog.Close>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
