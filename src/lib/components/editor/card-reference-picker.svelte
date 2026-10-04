<script lang="ts" module>
  import type { Task } from "../../../routes/type/task.svelte";
  export type CardReferenceOption = { task: Task; column_name: string };
</script>

<script lang="ts">
  import { m } from "$lib/paraglide/messages";
  import { tick } from "svelte";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import * as Command from "$lib/components/ui/command/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import Link2Icon from "@lucide/svelte/icons/link-2";
  import type { RichTextEditorHandle } from "./rich-text-surface.svelte";
  import { create_card_reference } from "../../../routes/utils/card-reference";
  let {
    editor,
    options,
    excludeTaskId = "",
    open = $bindable(false),
    triggerStart = $bindable<number | undefined>(undefined),
    compact = false,
    disabled = false,
  }: {
    editor: RichTextEditorHandle | null;
    options: CardReferenceOption[];
    excludeTaskId?: string;
    open?: boolean;
    triggerStart?: number;
    compact?: boolean;
    disabled?: boolean;
  } = $props();
  async function insert_card_reference(option: CardReferenceOption) {
    const handle = editor;
    const selection = handle?.getSelection();
    if (!handle || !selection) {
      return;
    }
    handle.replaceMarkdown(
      create_card_reference(option.task.title, option.task.id),
      { from: triggerStart ?? selection.from, to: selection.to },
    );
    open = false;
    triggerStart = undefined;
    await tick();
    handle.focus();
  }
</script>

<Popover.Root
  bind:open
  onOpenChange={(isOpen) => {
    if (!isOpen) {
      triggerStart = undefined;
    }
  }}
>
  <Popover.Trigger disabled={disabled || !editor}>
    {#snippet child({ props })}
      <Button
        {...props}
        type="button"
        variant="ghost"
        size={compact ? "icon-sm" : "sm"}
        class={compact ? "" : "h-7 select-none gap-1.5 px-2 text-xs"}
        aria-label={m.editor_reference_card()}
        title={m.editor_reference_card()}
      >
        <Link2Icon class="size-3.5" />
        {#if !compact}{m.editor_reference_card()}{/if}
      </Button>
    {/snippet}
  </Popover.Trigger>
  <Popover.Content class="w-80 p-0" align="end">
    <Command.Root>
      <Command.Input
        placeholder={m.task_search_cards_placeholder()}
        aria-label={m.task_search_cards()}
      />
      <Command.List class="max-h-64">
        <Command.Empty>{m.editor_no_cards()}</Command.Empty>
        <Command.Group value="cards">
          {#each options.filter((option) => option.task.id !== excludeTaskId) as option (option.task.id)}
            <Command.Item
              value={option.task.id}
              keywords={[option.task.title, option.column_name]}
              onSelect={() => void insert_card_reference(option)}
            >
              <div class="min-w-0">
                <div class="truncate">
                  {option.task.title}
                </div>
                <div class="truncate text-xs text-muted-foreground">
                  {option.column_name}
                </div>
              </div>
            </Command.Item>
          {/each}
        </Command.Group>
      </Command.List>
    </Command.Root>
  </Popover.Content>
</Popover.Root>
