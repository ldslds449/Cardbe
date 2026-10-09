<script lang="ts">
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { m } from "$lib/paraglide/messages.js";
  import { language, isLanguagePreference } from "$lib/i18n";
  import ThemeSettings from "./theme_settings.svelte";
  import { board } from "../../board.svelte";

  let { open = $bindable(false) }: { open: boolean } = $props();
  let saving = $state(false);
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

<Dialog.Root bind:open>
  <Dialog.Content
    class="flex h-[85dvh] max-h-160 flex-col overflow-hidden sm:max-w-md"
  >
    <Dialog.Header class="shrink-0"
      ><Dialog.Title>{m.settings_title()}</Dialog.Title></Dialog.Header
    >
    <ScrollArea
      type="always"
      class="min-h-0 flex-1"
      scrollbarYClasses="[&_[data-slot=scroll-area-thumb]]:bg-muted-foreground/50"
    >
      <div class="grid gap-4 py-1 ps-1 pe-4">
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
            <Select.Item value="system"
              >{m.settings_language_system()}</Select.Item
            >
            <Select.Item value="en">English</Select.Item>
            <Select.Item value="zh-TW">繁體中文</Select.Item>
          </Select.Content>
        </Select.Root>
        <ThemeSettings />
      </div>
    </ScrollArea>
  </Dialog.Content>
</Dialog.Root>
