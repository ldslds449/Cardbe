<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { onMount } from "svelte";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";
  import { Switch } from "$lib/components/ui/switch/index.js";
  import { m } from "$lib/paraglide/messages.js";
  import { language, isLanguagePreference } from "$lib/i18n";
  import { logger } from "$lib/logger";
  import { board } from "../../board.svelte";

  let {
    update_check_in_progress = false,
    onCheckForUpdates,
  }: {
    update_check_in_progress?: boolean;
    onCheckForUpdates: (manual?: boolean) => void | Promise<void>;
  } = $props();
  let saving = $state(false);
  let version = $state<string | null>(null);

  onMount(() => {
    getVersion()
      .then((value) => (version = value))
      .catch((error) => logger.warn("settings.version_load.failed", error));
  });

  async function changeLanguage(value: string) {
    if (!isLanguagePreference(value) || saving) {
      return;
    }
    saving = true;
    try {
      await board.set_language(value);
    } finally {
      saving = false;
    }
  }
</script>

<label for="settings-language" class="text-sm font-medium"
  >{m.settings_language()}</label
>
<Select.Root
  type="single"
  value={language.preference}
  onValueChange={changeLanguage}
  disabled={saving}
>
  <Select.Trigger id="settings-language" class="w-full">
    {language.preference === "en"
      ? "English"
      : language.preference === "zh-TW"
        ? "繁體中文"
        : m.settings_language_system()}
  </Select.Trigger>
  <Select.Content>
    <Select.Item value="system">{m.settings_language_system()}</Select.Item>
    <Select.Item value="en">English</Select.Item>
    <Select.Item value="zh-TW">繁體中文</Select.Item>
  </Select.Content>
</Select.Root>
<Separator />
<Field.Field orientation="horizontal">
  <Field.Content>
    <Field.Label for="settings-task-details"
      >{m.settings_task_details_default()}</Field.Label
    >
    <Field.Description id="settings-task-details-help"
      >{m.settings_task_details_default_help()}</Field.Description
    >
  </Field.Content>
  <Switch
    id="settings-task-details"
    checked={board.default_task_expand_mode}
    aria-describedby="settings-task-details-help"
    onCheckedChange={(expanded) =>
      void board.set_default_task_expand_mode(expanded)}
  />
</Field.Field>
<Separator />
<Field.Field orientation="horizontal">
  <Field.Content>
    <Field.Title>{m.settings_updates()}</Field.Title>
    <Field.Description
      >{version === null
        ? m.ui_loading()
        : m.settings_current_version({ version })}</Field.Description
    >
  </Field.Content>
  <Button
    variant="outline"
    disabled={update_check_in_progress}
    onclick={() => void onCheckForUpdates(true)}
  >
    {#if update_check_in_progress}
      <LoaderCircleIcon class="animate-spin" />
    {:else}
      <RefreshCwIcon />
    {/if}
    {m.ui_check_for_updates()}
  </Button>
</Field.Field>
