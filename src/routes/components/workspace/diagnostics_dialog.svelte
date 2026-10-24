<script lang="ts">
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "$lib/command-errors";
  import * as m from "$lib/paraglide/messages.js";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import DownloadIcon from "@lucide/svelte/icons/download";
  import FolderOpenIcon from "@lucide/svelte/icons/folder-open";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import ShieldCheckIcon from "@lucide/svelte/icons/shield-check";
  import { toast } from "svelte-sonner";

  import { Badge } from "$lib/components/ui/badge/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { logger } from "$lib/logger";
  import { platformLabel, type DiagnosticInfo } from "./diagnostics";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let diagnostics = $state<DiagnosticInfo | null>(null);
  let diagnostics_loading = $state(false);
  let content = $state<HTMLElement | null>(null);
  let diagnostics_error = $state<CommandError | null>(null);
  let was_loaded = false;

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
    diagnostics_error = null;
    try {
      diagnostics = await invoke<DiagnosticInfo>("get_diagnostics");
    } catch (error) {
      diagnostics_error = parseCommandError(error) ?? {
        code: "INTERNAL_ERROR",
      };
      logger.error("diagnostics.load.failed", error);
    } finally {
      diagnostics_loading = false;
    }
  }

  async function open_log_folder() {
    try {
      await invoke("open_log_folder");
    } catch (error) {
      logger.error("diagnostics.open_log_folder.failed", error);
      toast.error(translateCommandError(error));
    }
  }

  function debug_information(info: DiagnosticInfo): string {
    return [
      `Cardbe ${info.appVersion}`,
      `Build: ${info.buildCommit ?? "Unknown"}`,
      `Platform: ${platformLabel(info)}`,
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
      toast.error(translateCommandError(error));
    }
  }

  const close_button_class =
    "inline-flex h-9 items-center justify-center rounded-md border border-border bg-background px-3 text-sm font-medium shadow-xs transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/50";
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    bind:ref={content}
    tabindex={-1}
    class="flex max-h-[calc(100dvh-2rem)] flex-col overflow-hidden outline-none sm:max-w-[620px]"
    onOpenAutoFocus={(event) => {
      // Keep the close button from looking selected on every open.
      event.preventDefault();
      content?.focus({ preventScroll: true });
    }}
  >
    <Dialog.Header class="shrink-0 border-b pb-4">
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

    <ScrollArea class="min-h-0 flex-1" orientation="vertical">
      <div class="grid gap-4 pe-3">
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
            {translateCommandError(diagnostics_error)}
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
                  <div class="text-sm">{platformLabel(diagnostics)}</div>
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
                  <span
                    class="block text-xs font-normal text-primary-foreground/70"
                    >{m.ui_redacted_logs_and_metadata()}</span
                  >
                </span>
              </Button>
            </div>
          </div>
        {/if}
      </div>
    </ScrollArea>

    <Dialog.Footer class="shrink-0 border-t pt-4">
      <Dialog.Close class={close_button_class}>{m.common_close()}</Dialog.Close>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
