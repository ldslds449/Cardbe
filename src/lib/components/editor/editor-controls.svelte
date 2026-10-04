<script lang="ts">
  import { m } from "$lib/paraglide/messages";
  import { tick } from "svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import Settings2Icon from "@lucide/svelte/icons/settings-2";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import Combobox from "../../../routes/components/combobox.svelte";
  import { editorMode } from "./mode";
  import { codeLanguages } from "./extensions/code-highlight";

  let {
    previewing = $bindable(false),
    language = null,
    disabled = false,
    compact = false,
    onlanguagechange,
  }: {
    previewing?: boolean;
    language?: string | null;
    disabled?: boolean;
    compact?: boolean;
    onlanguagechange?: (language: string | null) => void;
  } = $props();
  let trigger = $state<HTMLButtonElement | null>(null);
  let pendingMode: "rich" | "plain" | null = null;
  const languages = $derived([
    { value: "text", label: m.editor_plain_code() },
    ...(language && language !== "text" && !codeLanguages.includes(language)
      ? [{ value: language, label: language }]
      : []),
    ...codeLanguages.map((value) => ({ value, label: value })),
  ]);
  function restoreFocus(event: Event) {
    event.preventDefault();
    void tick().then(() => {
      trigger?.focus({ preventScroll: true });
      // Replace the editor only after the dialog remembers the stable trigger.
      if (pendingMode) {
        editorMode.set(pendingMode);
      }
      pendingMode = null;
    });
  }
  function setMode(value: string) {
    if (value === "rich" || value === "plain") {
      pendingMode = value;
    }
  }
</script>

<div
  class="flex h-8 min-w-0 items-center gap-2"
  role="group"
  aria-label={m.editor_controls()}
>
  <DropdownMenu.Root>
    <DropdownMenu.Trigger>
      {#snippet child({ props })}
        <Button
          {...props}
          bind:ref={trigger}
          type="button"
          size={compact ? "icon-sm" : "sm"}
          variant={compact ? "ghost" : "outline"}
          class={compact ? "" : "h-7 w-28 justify-between text-xs"}
          aria-label={compact ? m.editor_settings() : m.editor_mode()}
          title={compact ? m.editor_settings() : undefined}
        >
          {#if compact}<Settings2Icon />{:else}{$editorMode === "rich"
              ? m.editor_rich_text()
              : m.editor_plain_text()}<ChevronDownIcon />{/if}
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content
      align={compact ? "end" : "start"}
      onCloseAutoFocus={restoreFocus}
    >
      <DropdownMenu.Label>{m.editor_mode()}</DropdownMenu.Label>
      <DropdownMenu.RadioGroup value={$editorMode} onValueChange={setMode}>
        <DropdownMenu.RadioItem value="rich"
          >{m.editor_rich_text()}</DropdownMenu.RadioItem
        >
        <DropdownMenu.RadioItem value="plain"
          >{m.editor_plain_text()}</DropdownMenu.RadioItem
        >
      </DropdownMenu.RadioGroup>
      {#if compact && $editorMode === "plain"}
        <DropdownMenu.Separator />
        <DropdownMenu.RadioGroup
          value={previewing ? "preview" : "edit"}
          onValueChange={(value) => (previewing = value === "preview")}
        >
          <DropdownMenu.RadioItem value="edit"
            >{m.editor_edit()}</DropdownMenu.RadioItem
          >
          <DropdownMenu.RadioItem value="preview"
            >{m.editor_preview()}</DropdownMenu.RadioItem
          >
        </DropdownMenu.RadioGroup>
      {/if}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
  {#if !compact && $editorMode === "plain"}
    <div
      class="flex items-center gap-1"
      role="group"
      aria-label={m.editor_plain_text_view()}
    >
      <Button
        type="button"
        size="sm"
        class="h-7 px-2 text-xs"
        variant={previewing ? "ghost" : "secondary"}
        aria-pressed={!previewing}
        onclick={() => (previewing = false)}>{m.editor_edit()}</Button
      >
      <Button
        type="button"
        size="sm"
        class="h-7 px-2 text-xs"
        variant={previewing ? "secondary" : "ghost"}
        aria-pressed={previewing}
        onclick={() => (previewing = true)}>{m.editor_preview()}</Button
      >
    </div>
  {/if}
  {#if $editorMode === "rich" && language !== null && !disabled}
    <Combobox
      items={languages}
      selected_value={language || "text"}
      select_placeholder={m.editor_code_language()}
      search_placeholder={m.editor_search_language()}
      search_label={m.editor_search_code_language()}
      class={compact ? "h-7 w-24 px-2 text-xs" : "h-7 w-36 px-2 text-xs"}
      onselect={(value) => onlanguagechange?.(value === "text" ? null : value)}
    />
  {/if}
</div>
