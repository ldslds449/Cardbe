<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { m } from "$lib/paraglide/messages.js";
  import { language, isLanguagePreference } from "$lib/i18n";
  import { toast } from "svelte-sonner";
  import { board } from "../../board.svelte";

  let { open = $bindable(false) }: { open: boolean } = $props();
  let saving = $state(false);
  async function changeLanguage(value: string) {
    if (!isLanguagePreference(value) || saving) {
      return;
    }
    saving = true;
    try {
      if (!(await board.set_language(value))) {
        toast.error(m.settings_language_save_error());
      }
    } finally {
      saving = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header
      ><Dialog.Title>{m.settings_title()}</Dialog.Title></Dialog.Header
    >
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
  </Dialog.Content>
</Dialog.Root>
