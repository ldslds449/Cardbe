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
      <AlertDialog.Title>Import board as new?</AlertDialog.Title>
      <AlertDialog.Description>
        A new board named “{candidate?.board_name ?? "Imported board"}” will be
        created. Your existing boards will not be changed.
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
    {/if}

    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={importing}>Cancel</AlertDialog.Cancel>
      <Button disabled={importing} onclick={() => void onConfirm()}>
        {show_importing ? "Importing..." : "Import as new board"}
      </Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
