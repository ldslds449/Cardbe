<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import BellIcon from "@lucide/svelte/icons/bell";
  import KeyboardIcon from "@lucide/svelte/icons/keyboard";
  import NetworkIcon from "@lucide/svelte/icons/network";
  import PaletteIcon from "@lucide/svelte/icons/palette";
  import PuzzleIcon from "@lucide/svelte/icons/puzzle";
  import ServerIcon from "@lucide/svelte/icons/server";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import * as Select from "#lib/components/ui/select/index.js";
  import * as Tabs from "#lib/components/ui/tabs/index.js";
  import { m } from "#lib/paraglide/messages.js";
  import { confirmUnsavedChanges } from "#lib/unsaved-changes.js";
  import GeneralSettings from "./general_settings.svelte";
  import GlobalShortcutSettings from "./global_shortcut_settings.svelte";
  import McpServerSettings from "./mcp_server_settings.svelte";
  import NetworkSettings from "./network_settings.svelte";
  import NotificationSettings from "./notification_settings.svelte";
  import ThemeSettings from "./theme_settings.svelte";
  import PluginSettings from "./plugin_settings.svelte";
  import {
    isSettingsSection,
    settingsDialog,
    type SettingsSection,
  } from "./settings-dialog.svelte";

  let {
    update_check_in_progress = false,
    onCheckForUpdates,
    onReviewPluginDomain,
  }: {
    update_check_in_progress?: boolean;
    onCheckForUpdates: (manual?: boolean) => void | Promise<void>;
    onReviewPluginDomain: (id: string) => void;
  } = $props();
  let network_dirty = $state(false);
  let network_busy = $state(false);
  let plugins_busy = $state(false);
  let content = $state<HTMLElement | null>(null);

  const sections: {
    id: SettingsSection;
    icon: Component;
    label: () => string;
  }[] = [
    { id: "general", icon: SettingsIcon, label: m.settings_tab_general },
    { id: "theme", icon: PaletteIcon, label: m.settings_tab_theme },
    {
      id: "notifications",
      icon: BellIcon,
      label: m.settings_tab_notifications,
    },
    { id: "shortcuts", icon: KeyboardIcon, label: m.settings_tab_shortcuts },
    { id: "sharing", icon: NetworkIcon, label: m.settings_tab_sharing },
    { id: "plugins", icon: PuzzleIcon, label: m.plugin_title },
    { id: "mcp", icon: ServerIcon, label: m.settings_tab_mcp },
  ];
  const current = $derived(
    sections.find((section) => section.id === settingsDialog.section)!,
  );

  function set_open(value: boolean) {
    if (value || !network_dirty) {
      settingsDialog.open = value;
      return;
    }
    void confirmUnsavedChanges(() => network_dirty).then((discard) => {
      if (discard) {
        settingsDialog.open = false;
      }
    });
  }
</script>

{#snippet scrollable(content: Snippet)}
  <ScrollArea
    type="always"
    class="h-full"
    scrollbarYClasses="[&_[data-slot=scroll-area-thumb]]:bg-muted-foreground/50"
  >
    <div class="grid gap-4 py-1 ps-1 pe-4">{@render content()}</div>
  </ScrollArea>
{/snippet}

{#snippet general()}
  <GeneralSettings {update_check_in_progress} {onCheckForUpdates} />
{/snippet}

{#snippet theme()}
  <ThemeSettings />
{/snippet}

{#snippet notifications()}
  <NotificationSettings />
{/snippet}

{#snippet shortcuts()}
  <GlobalShortcutSettings />
{/snippet}

{#snippet mcp()}
  <McpServerSettings />
{/snippet}

<Dialog.Root
  bind:open={() => settingsDialog.open, set_open}
  busy={network_busy || plugins_busy}
>
  <Dialog.Content
    bind:ref={content}
    tabindex={-1}
    class="flex h-[85dvh] max-h-200 w-[calc(100vw-2rem)] flex-col overflow-hidden outline-none sm:max-w-4xl"
    onOpenAutoFocus={(event) => {
      // Focusing the first section would draw its focus ring on every open.
      event.preventDefault();
      content?.focus({ preventScroll: true });
    }}
  >
    <Dialog.Header class="shrink-0"
      ><Dialog.Title>{m.settings_title()}</Dialog.Title></Dialog.Header
    >
    <!-- Inactive panels stay mounted (hidden), so drafts survive section switches. -->
    <Tabs.Root
      bind:value={settingsDialog.section}
      orientation="vertical"
      class="min-h-0 flex-1 gap-4 sm:flex-row"
    >
      <Select.Root
        type="single"
        value={settingsDialog.section}
        onValueChange={(value) => {
          if (isSettingsSection(value)) {
            settingsDialog.section = value;
          }
        }}
      >
        <Select.Trigger
          class="w-full shrink-0 sm:hidden"
          aria-label={m.settings_section()}
          ><current.icon />{current.label()}</Select.Trigger
        >
        <Select.Content>
          {#each sections as section (section.id)}
            <Select.Item value={section.id}>{section.label()}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
      <ScrollArea class="hidden w-44 shrink-0 sm:block">
        <Tabs.List
          aria-label={m.settings_section()}
          class="flex h-auto w-full flex-col items-stretch gap-1 bg-transparent p-0"
        >
          {#each sections as section (section.id)}
            <Tabs.Trigger
              value={section.id}
              class="h-9 flex-none justify-start px-3 data-[state=active]:bg-muted data-[state=active]:shadow-none dark:data-[state=active]:border-transparent dark:data-[state=active]:bg-muted"
              ><section.icon />{section.label()}</Tabs.Trigger
            >
          {/each}
        </Tabs.List>
      </ScrollArea>
      <Tabs.Content value="general" class="min-h-0"
        >{@render scrollable(general)}</Tabs.Content
      >
      <Tabs.Content value="theme" class="min-h-0"
        >{@render scrollable(theme)}</Tabs.Content
      >
      <Tabs.Content value="notifications" class="min-h-0"
        >{@render scrollable(notifications)}</Tabs.Content
      >
      <Tabs.Content value="shortcuts" class="min-h-0"
        >{@render scrollable(shortcuts)}</Tabs.Content
      >
      <Tabs.Content value="sharing" class="min-h-0">
        <NetworkSettings
          open={settingsDialog.open}
          active={settingsDialog.section === "sharing"}
          bind:dirty={network_dirty}
          bind:busy={network_busy}
        />
      </Tabs.Content>
      <Tabs.Content value="plugins" class="min-h-0">
        <PluginSettings
          open={settingsDialog.open && settingsDialog.section === "plugins"}
          bind:busy={plugins_busy}
          onReviewDomain={onReviewPluginDomain}
        />
      </Tabs.Content>
      <Tabs.Content value="mcp" class="min-h-0"
        >{@render scrollable(mcp)}</Tabs.Content
      >
    </Tabs.Root>
  </Dialog.Content>
</Dialog.Root>
