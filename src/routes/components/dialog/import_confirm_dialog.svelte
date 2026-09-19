<script lang="ts">
  import { Button } from "$lib/components/ui/button/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";

  import type { ImportCandidate } from "../../board.svelte";

  let {
    open = $bindable(),
    candidate,
    importing,
    show_importing,
    onConfirm,
  }: {
    open: boolean;
    candidate: ImportCandidate | null;
    importing: boolean;
    show_importing: boolean;
    onConfirm: () => void | Promise<void>;
  } = $props();
</script>

<AlertDialog.Root bind:open>
  <AlertDialog.Content class="sm:max-w-md">
    <AlertDialog.Header>
      <AlertDialog.Title>Replace current data?</AlertDialog.Title>
      <AlertDialog.Description>
        Importing will replace the current board, archives, and task templates.
      </AlertDialog.Description>
    </AlertDialog.Header>

    {#if candidate}
      <div class="grid grid-cols-4 gap-2 py-2">
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {candidate.summary.columns}
          </div>
          <div class="text-xs text-muted-foreground">Columns</div>
        </div>
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {candidate.summary.tasks}
          </div>
          <div class="text-xs text-muted-foreground">Active tasks</div>
        </div>
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {candidate.summary.archives}
          </div>
          <div class="text-xs text-muted-foreground">Archived</div>
        </div>
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {candidate.summary.templates}
          </div>
          <div class="text-xs text-muted-foreground">Templates</div>
        </div>
      </div>

      <div class="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm">
        A backup of your current data will be created before anything is replaced.
      </div>
    {/if}

    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={importing}>Cancel</AlertDialog.Cancel>
      <Button disabled={importing} onclick={() => void onConfirm()}>
        {show_importing ? "Importing..." : "Import data"}
      </Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
