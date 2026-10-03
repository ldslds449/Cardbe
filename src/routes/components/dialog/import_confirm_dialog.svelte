<script lang="ts">
  import { formatNumber } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
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
      <AlertDialog.Title>{m.ui_import_board_as_new()}</AlertDialog.Title>
      <AlertDialog.Description>
        {m.import_board_description({
          name: candidate?.board_name ?? m.ui_imported_board(),
        })}
      </AlertDialog.Description>
    </AlertDialog.Header>

    {#if candidate}
      <div class="grid grid-cols-4 gap-2 py-2">
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {formatNumber(candidate.summary.columns)}
          </div>
          <div class="text-xs text-muted-foreground">{m.ui_columns()}</div>
        </div>
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {formatNumber(candidate.summary.tasks)}
          </div>
          <div class="text-xs text-muted-foreground">{m.ui_active_tasks()}</div>
        </div>
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {formatNumber(candidate.summary.archives)}
          </div>
          <div class="text-xs text-muted-foreground">
            {m.explorer_archived()}
          </div>
        </div>
        <div class="rounded-lg border bg-muted/40 p-3 text-center">
          <div class="text-2xl font-semibold tabular-nums">
            {formatNumber(candidate.summary.templates)}
          </div>
          <div class="text-xs text-muted-foreground">{m.ui_templates()}</div>
        </div>
      </div>
    {/if}

    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={importing}
        >{m.common_cancel()}</AlertDialog.Cancel
      >
      <Button disabled={importing} onclick={() => void onConfirm()}>
        {show_importing ? m.ui_importing() : m.ui_import_as_new_board()}
      </Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
