<script lang="ts">
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
      <Dialog.Title>Share Task</Dialog.Title>
      <Dialog.Description>
        Copy this text and send it to another Cardbe user.
      </Dialog.Description>
    </Dialog.Header>
    <Textarea
      value={share_text}
      readonly
      aria-label="Task share text"
      class="min-h-40 resize-y break-all font-mono text-xs"
      onclick={(event) => event.currentTarget.select()}
    />
    <Dialog.Footer>
      <Button variant="outline" onclick={() => { export_open = false; }}
        >Close</Button
      >
      <Button onclick={() => void onCopy()}>Copy Text</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={import_open}>
  <Dialog.Content class="sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title>Import Task</Dialog.Title>
      <Dialog.Description>
        {#if import_target_column}
          Paste task sharing text to import it into “{import_target_column}”.
        {:else}
          Paste task sharing text from another Cardbe user.
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
        aria-label="Task sharing text"
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
          onclick={() => { import_open = false; }}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={!import_text.trim()}>Continue</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
