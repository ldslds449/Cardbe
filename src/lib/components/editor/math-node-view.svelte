<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import { Label } from "#lib/components/ui/label/index.js";
  import { Textarea } from "#lib/components/ui/textarea/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import MathContent from "./math-content.svelte";
  import { renderMath } from "./math";
  import { onMount } from "svelte";
  let {
    source,
    display,
    editable,
    save,
    focus,
  }: {
    source: string;
    display: boolean;
    editable: () => boolean;
    save: (source: string) => void;
    focus: () => void;
  } = $props();
  const id = $props.id();
  let open = $state(false);
  let draft = $state("");
  const html = $derived(renderMath(source, display));
  onMount(() => {
    if (!source.trim() && editable()) {
      open = true;
    }
  });
</script>

{#snippet trigger()}
  <Dialog.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        type="button"
        variant="ghost"
        class="math-trigger h-auto max-w-full px-1 py-0 font-normal"
        disabled={!editable()}
        aria-label={m.editor_math_edit_label({ formula: source })}
        onclick={() => {
          draft = source;
        }}
      >
        {#if !source.trim()}{m.editor_math_source()}{:else if html}{@html html}{:else}<span
            class="whitespace-pre-wrap font-mono">{source}</span
          >{/if}
      </Button>
    {/snippet}
  </Dialog.Trigger>
{/snippet}

<span contenteditable="false" class="math-node" class:math-node-block={display}>
  <Dialog.Root bind:open>
    {#if display}
      <ScrollArea
        orientation="horizontal"
        class="math-scroll min-w-0 max-w-full overflow-hidden"
        >{@render trigger()}</ScrollArea
      >
    {:else}
      {@render trigger()}
    {/if}
    <Dialog.Content
      onCloseAutoFocus={(event) => {
        event.preventDefault();
        focus();
      }}
    >
      <Dialog.Header>
        <Dialog.Title>{m.editor_math_edit()}</Dialog.Title>
        <Dialog.Description>{m.editor_math_help()}</Dialog.Description>
      </Dialog.Header>
      <form
        class="grid min-w-0 gap-4"
        onsubmit={(event) => {
          event.preventDefault();
          if (draft.trim()) {
            save(draft);
            open = false;
          }
        }}
      >
        <Label for={id}>{m.editor_math_source()}</Label>
        <ScrollArea
          orientation="vertical"
          class="editor-auto-height max-h-48 overflow-hidden rounded-md border"
        >
          <Textarea
            {id}
            bind:value={draft}
            required
            class="min-h-24 resize-none overflow-hidden border-0 font-mono"
            style="field-sizing: content"
          />
        </ScrollArea>
        <MathContent source={draft} {display} />
        <Dialog.Footer>
          <Button
            type="button"
            variant="outline"
            onclick={() => {
              open = false;
            }}>{m.common_cancel()}</Button
          >
          <Button type="submit" disabled={!draft.trim()}
            >{m.common_save()}</Button
          >
        </Dialog.Footer>
      </form>
    </Dialog.Content>
  </Dialog.Root>
</span>
