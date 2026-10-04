<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import ExternalLinkIcon from "@lucide/svelte/icons/external-link";
  import FileTextIcon from "@lucide/svelte/icons/file-text";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import GithubIcon from "@lucide/svelte/icons/github";
  import InfoIcon from "@lucide/svelte/icons/info";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
  import { toast } from "svelte-sonner";

  import { Badge } from "$lib/components/ui/badge/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { logger } from "$lib/logger";
  import updateConfig from "../../../config/update.json";

  type DiagnosticInfo = {
    appVersion: string;
    buildCommit: string | null;
    os: string;
    architecture: string;
    logDirectory: string;
  };

  let {
    about_open = $bindable(false),
    diagnostics_open = $bindable(false),
    update_check_in_progress = false,
    onCheckForUpdates,
  }: {
    about_open?: boolean;
    diagnostics_open?: boolean;
    update_check_in_progress?: boolean;
    onCheckForUpdates: (manual?: boolean) => void | Promise<void>;
  } = $props();

  let diagnostics = $state<DiagnosticInfo | null>(null);
  let diagnostics_loading = $state(false);
  let diagnostics_error = $state<string | null>(null);
  let dialogs_were_loaded = false;

  const github_url = `https://github.com/${updateConfig.githubOwner}/${updateConfig.githubRepository}`;
  const license_url = `${github_url}/blob/main/LICENSE`;

  $effect(() => {
    const open = about_open || diagnostics_open;
    if (open && !dialogs_were_loaded) {
      dialogs_were_loaded = true;
      void load_diagnostics();
    } else if (!open) {
      dialogs_were_loaded = false;
    }
  });

  async function load_diagnostics() {
    diagnostics_loading = true;
    diagnostics_error = null;
    try {
      diagnostics = await invoke<DiagnosticInfo>("get_diagnostics");
    } catch (error) {
      diagnostics_error = m.ui_couldn_t_load_diagnostic_information();
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
      toast.error(m.ui_couldn_t_open_the_link());
    }
  }

  async function open_log_folder() {
    try {
      await invoke("open_log_folder");
    } catch (error) {
      logger.error("diagnostics.open_log_folder.failed", error);
      toast.error(m.ui_couldn_t_open_the_log_folder());
    }
  }

  function platform_label(info: DiagnosticInfo): string {
    const os =
      {
        windows: "Windows",
        macos: "macOS",
        linux: "Linux",
        android: "Android",
        ios: "iOS",
      }[info.os] ?? info.os;
    return `${os} ${info.architecture}`;
  }

  function debug_information(info: DiagnosticInfo): string {
    return [
      `Cardbe ${info.appVersion}`,
      `Build: ${info.buildCommit ?? "Unknown"}`,
      `Platform: ${platform_label(info)}`,
    ].join("\n");
  }

  async function copy_debug_information() {
    if (!diagnostics) {
      return;
    }
    try {
      await navigator.clipboard.writeText(debug_information(diagnostics));
      toast.success(m.ui_debug_information_copied());
    } catch (error) {
      logger.error("diagnostics.copy.failed", error);
      toast.error(m.ui_couldn_t_copy_debug_information());
    }
  }

  async function export_debug_information() {
    try {
      const date = new Date().toISOString().slice(0, 10);
      const destination = await save({
        defaultPath: `cardbe-debug-${date}.zip`,
        filters: [{ name: m.diagnostics_zip_archive(), extensions: ["zip"] }],
      });
      if (!destination) {
        return;
      }

      await invoke("export_debug_information", { destination });
      toast.success(m.ui_debug_information_exported());
    } catch (error) {
      logger.error("diagnostics.export.failed", error);
      toast.error(m.ui_couldn_t_export_debug_information());
    }
  }

  const close_button_class =
    "inline-flex h-9 items-center justify-center rounded-md border border-border bg-background px-3 text-sm font-medium shadow-xs transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/50";
</script>

<Dialog.Root bind:open={about_open}>
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

<Dialog.Root bind:open={diagnostics_open}>
  <Dialog.Content
    class="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-[620px]"
  >
    <Dialog.Header class="border-b pb-4">
      <div class="flex items-start justify-between gap-4 pr-8">
        <div class="space-y-1">
          <Dialog.Title>{m.ui_diagnostics()}</Dialog.Title>
          <Dialog.Description>
            {m.ui_review_safe_app_details_and_prepare_information_for_support()}
          </Dialog.Description>
        </div>
        <Badge variant="outline" class="mt-0.5 shrink-0 gap-1.5">
          <ShieldCheckIcon class="size-3.5 text-emerald-600" />
          {m.ui_redacted_export()}
        </Badge>
      </div>
    </Dialog.Header>

    {#if diagnostics_loading}
      <div
        class="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground"
      >
        <LoaderCircleIcon class="size-4 animate-spin" />
        {m.ui_loading_diagnostic_information()}
      </div>
    {:else if diagnostics_error}
      <div
        class="rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive"
      >
        {diagnostics_error}
      </div>
    {:else if diagnostics}
      <div class="grid gap-3 sm:grid-cols-2">
        <Card.Root class="gap-0 py-0">
          <Card.Header class="border-b py-4">
            <Card.Title class="text-sm">{m.ui_app_details()}</Card.Title>
          </Card.Header>
          <Card.Content class="grid gap-4 py-4">
            <div class="space-y-1">
              <div class="text-xs font-medium text-muted-foreground">
                {m.ui_version()}
              </div>
              <div class="font-mono text-sm">{diagnostics.appVersion}</div>
            </div>
            <div class="space-y-1">
              <div class="text-xs font-medium text-muted-foreground">
                {m.ui_build()}
              </div>
              <div class="font-mono text-sm">
                {diagnostics.buildCommit ?? "Unknown"}
              </div>
            </div>
            <div class="space-y-1">
              <div class="text-xs font-medium text-muted-foreground">
                {m.ui_platform()}
              </div>
              <div class="text-sm">{platform_label(diagnostics)}</div>
            </div>
          </Card.Content>
        </Card.Root>

        <Card.Root class="gap-0 py-0">
          <Card.Header class="border-b py-4">
            <Card.Title class="text-sm">{m.ui_logs()}</Card.Title>
          </Card.Header>
          <Card.Content class="space-y-4 py-4">
            <div class="space-y-1">
              <div class="text-xs font-medium text-muted-foreground">
                {m.ui_log_location()}
              </div>
              <div
                class="rounded-md bg-muted/50 px-3 py-2 font-mono text-xs break-all"
              >
                {diagnostics.logDirectory}
              </div>
            </div>
            <Button
              variant="outline"
              class="w-full"
              onclick={() => void open_log_folder()}
            >
              <FolderOpenIcon class="size-4" />
              {m.ui_open_log_folder()}
            </Button>
          </Card.Content>
        </Card.Root>
      </div>

      <div class="space-y-3">
        <div>
          <h3 class="text-sm font-medium">{m.ui_support_tools()}</h3>
          <p class="text-sm text-muted-foreground">
            {m.help_export_description()}
          </p>
        </div>
        <div class="grid gap-2 sm:grid-cols-2">
          <Button
            variant="outline"
            class="h-auto min-h-14 justify-start gap-3 whitespace-normal px-3 py-3 text-left"
            onclick={() => void copy_debug_information()}
          >
            <CopyIcon class="size-4 shrink-0" />
            <span>
              <span class="block">{m.ui_copy_debug_information()}</span>
              <span class="block text-xs font-normal text-muted-foreground"
                >{m.ui_three_line_issue_summary()}</span
              >
            </span>
          </Button>
          <Button
            class="h-auto min-h-14 justify-start gap-3 whitespace-normal px-3 py-3 text-left"
            onclick={() => void export_debug_information()}
          >
            <DownloadIcon class="size-4 shrink-0" />
            <span>
              <span class="block">{m.ui_export_debug_information()}</span>
              <span class="block text-xs font-normal text-primary-foreground/70"
                >{m.ui_redacted_logs_and_metadata()}</span
              >
            </span>
          </Button>
        </div>
      </div>
    {/if}

    <Dialog.Footer class="border-t pt-4">
      <Dialog.Close class={close_button_class}>{m.common_close()}</Dialog.Close>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
