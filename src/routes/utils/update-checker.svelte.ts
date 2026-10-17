import { m } from "$lib/paraglide/messages.js";
import { translateCommandError } from "$lib/command-errors";
import { logger } from "$lib/logger";
import { getName, getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import {
  markUpdateCheckSuccessful,
  shouldCheckForUpdate,
  updateCheckErrorMessage,
} from "./update";

// Startup and manual checks for a newer GitHub release.
export class UpdateChecker {
  app_name = $state("");
  in_progress = $state(false);
  private app_name_promise: Promise<string> | undefined;

  load_app_name(): Promise<string> {
    this.app_name_promise ??= getName().then((name) => {
      this.app_name = name;
      return name;
    });
    return this.app_name_promise;
  }

  async check(manual = false) {
    if (this.in_progress) {
      return;
    }

    try {
      if (!manual && !shouldCheckForUpdate(window.localStorage)) {
        return;
      }
      this.in_progress = true;

      const [resolved_app_name, current_version] = await Promise.all([
        this.load_app_name(),
        getVersion(),
      ]);
      const update = await invoke<{ version: string; url: string } | null>(
        "check_for_update",
        {
          currentVersion: current_version,
        },
      );
      markUpdateCheckSuccessful(window.localStorage);
      if (!update) {
        if (manual) {
          toast.success(m.update_current({ name: resolved_app_name }));
        }
        return;
      }

      toast.info(
        m.update_available({
          name: resolved_app_name,
          version: update.version,
        }),
        {
          id: `app-update-${update.version}`,
          description: m.ui_download_the_new_version_from_github_releases(),
          duration: Infinity,
          closeButton: true,
          action: {
            label: m.ui_view_release(),
            onClick: () => {
              void invoke("open_external_url", { url: update.url }).catch(
                (error) => {
                  logger.warn("update.open_release.failed", error);
                  toast.error(translateCommandError(error));
                },
              );
            },
          },
          cancel: {
            label: m.ui_later(),
            onClick: () => {},
          },
        },
      );
    } catch (error) {
      logger.warn("update.manual_check.failed", error);
      // Automatic update checks should never interrupt normal app usage.
      if (manual) {
        toast.error(m.ui_couldn_t_check_for_updates(), {
          description: updateCheckErrorMessage(error),
          duration: 10_000,
          closeButton: true,
        });
      }
    } finally {
      this.in_progress = false;
    }
  }
}
