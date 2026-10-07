<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import { dev } from "$app/environment";
  import "../app.css";
  import Sonner from "$lib/components/ui/sonner/sonner.svelte";
  import UnsavedChangesDialog from "$lib/components/unsaved-changes-dialog.svelte";
  import { onMount } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    applyLanguagePreference,
    getLocale,
    refreshSystemLocale,
  } from "$lib/i18n";
  import { installGlobalErrorLogging, logger } from "$lib/logger";

  let { children } = $props();

  $effect(() => {
    document.documentElement.lang = getLocale();
    document.documentElement.dir = "ltr";
    document
      .getElementById("app-startup")
      ?.setAttribute("aria-label", m.app_loading());
  });

  $effect(() => {
    if (isTauri() && window.location.pathname === "/") {
      void invoke("set_desktop_menu_labels", {
        labels: [
          m.task_new(),
          m.ui_new_note(),
          m.tray_enable_shortcuts(),
          m.tray_show(),
          m.tray_quit(),
        ],
      }).catch((error) => logger.warn("desktop.menu_language.failed", error));
    }
  });

  onMount(() => {
    let disposed = false;
    let unlistenLanguage: UnlistenFn | undefined;
    let languageRevision = 0;
    const loadLanguage = () => {
      const revision = ++languageRevision;
      void invoke<{ language?: string }>("get_settings")
        .then((settings) => {
          if (!disposed && revision === languageRevision) {
            applyLanguagePreference(settings.language);
          }
        })
        .catch((error) => logger.warn("settings.language_load.failed", error));
    };
    if (isTauri()) {
      void listen("cardbe:language-changed", (event) => {
        languageRevision++;
        applyLanguagePreference(event.payload);
      })
        .then((unlisten) => {
          if (disposed) {
            unlisten();
          } else {
            unlistenLanguage = unlisten;
            loadLanguage();
          }
        })
        .catch((error) => {
          logger.warn("settings.language_listen.failed", error);
          if (!disposed) {
            loadLanguage();
          }
        });
      window.addEventListener("focus", loadLanguage);
    }
    const cleanupLanguage = () => {
      disposed = true;
      unlistenLanguage?.();
      window.removeEventListener("focus", loadLanguage);
    };
    const uninstallErrorLogging = installGlobalErrorLogging();
    logger.info("app.mounted");
    const dismiss_startup = () =>
      document.getElementById("app-startup")?.classList.add("is-ready");
    const waits_for_workspace = window.location.pathname === "/";
    if (waits_for_workspace) {
      window.addEventListener("cardbe:workspace-ready", dismiss_startup, {
        once: true,
      });
    } else {
      dismiss_startup();
    }

    if (dev) {
      return () => {
        cleanupLanguage();
        uninstallErrorLogging();
        window.removeEventListener("cardbe:workspace-ready", dismiss_startup);
      };
    }

    const preventBrowserContextMenu = (event: MouseEvent) =>
      event.preventDefault();
    document.addEventListener("contextmenu", preventBrowserContextMenu);

    return () => {
      cleanupLanguage();
      uninstallErrorLogging();
      window.removeEventListener("cardbe:workspace-ready", dismiss_startup);
      document.removeEventListener("contextmenu", preventBrowserContextMenu);
    };
  });
</script>

<svelte:window onlanguagechange={refreshSystemLocale} />

{@render children()}
<UnsavedChangesDialog />
<Sonner expand closeButton closeButtonAriaLabel={m.notification_close()} />
