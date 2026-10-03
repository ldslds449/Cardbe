<script lang="ts">
  import * as m from "$lib/paraglide/messages.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Textarea } from "$lib/components/ui/textarea/index.js";

  let {
    export_open = $bindable(),
    import_open = $bindable(),
    share_text,
    import_text = $bindable(),
    import_error,
    import_target_column,
    onCopy,
    onImport,
  }: {
    export_open: boolean;
    import_open: boolean;
    share_text: string;
    import_text: string;
    import_error?: string;
    import_target_column?: string;
    onCopy: () => void | Promise<void>;
    onImport: () => void;
  } = $props();
</script>

<Dialog.Root bind:open={export_open}>
  <Dialog.Content class="sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>{m.task_share()}</Dialog.Title>
      <Dialog.Description>
        {m.ui_copy_this_text_and_send_it_to_another_cardbe_user()}
      </Dialog.Description>
    </Dialog.Header>
    <Textarea
      value={share_text}
      readonly
      aria-label={m.ui_task_share_text()}
      class="min-h-40 resize-y break-all font-mono text-xs"
      onclick={(event) => event.currentTarget.select()}
    />
    <Dialog.Footer>
      <Button
        variant="outline"
        onclick={() => {
          export_open = false;
        }}>{m.common_close()}</Button
      >
      <Button onclick={() => void onCopy()}>{m.ui_copy_text()}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={import_open}>
  <Dialog.Content class="sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>{m.task_import()}</Dialog.Title>
      <Dialog.Description>
        {#if import_target_column}
          {m.import_task_column_description({ name: import_target_column })}
        {:else}
          {m.ui_paste_task_sharing_text_from_another_cardbe_user()}
        {/if}
      </Dialog.Description>
    </Dialog.Header>
    <form
      class="space-y-3"
      onsubmit={(event) => {
        event.preventDefault();
        onImport();
      }}
    >
      <Textarea
        bind:value={import_text}
        aria-label={m.ui_task_sharing_text()}
        placeholder="cardbe-task:v1:..."
        class="min-h-40 resize-y break-all font-mono text-xs"
      />
      {#if import_error}
        <p class="text-sm text-destructive" role="alert">{import_error}</p>
      {/if}
      <Dialog.Footer>
        <Button
          type="button"
          variant="outline"
          onclick={() => {
            import_open = false;
          }}
        >
          {m.common_cancel()}
        </Button>
        <Button type="submit" disabled={!import_text.trim()}
          >{m.ui_continue()}</Button
        >
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
