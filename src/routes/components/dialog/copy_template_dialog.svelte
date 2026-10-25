<script lang="ts">
  import * as m from "#lib/paraglide/messages.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as Select from "#lib/components/ui/select/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { board } from "../../board.svelte";
  import type { TaskTemplate } from "../../type/task.svelte";

  let {
    template,
    source_id,
    onClose,
  }: { template: TaskTemplate; source_id: number; onClose: () => void } =
    $props();
  let open = $state(true);
  let target = $state("");
  let copying = $state(false);
  const targets = $derived(
    board.boards.filter(
      (b) => b.id !== source_id && b.shared_role !== "viewer",
    ),
  );
  const valid = $derived(
    board.active_board_id === source_id &&
      targets.some((b) => String(b.id) === target),
  );

  $effect(() => {
    if (!open) {
      onClose();
    }
  });

  async function copy() {
    if (copying || !valid) {
      return;
    }
    copying = true;
    if (await board.copy_task_template_to_board(template.id, Number(target))) {
      open = false;
    }
    copying = false;
  }
</script>

<Dialog.Root bind:open busy={copying}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{m.template_copy_board_title()}</Dialog.Title>
      <Dialog.Description
        >{m.template_copy_board_description({
          name: template.name,
        })}</Dialog.Description
      >
    </Dialog.Header>
    <div class="space-y-2">
      <p class="text-sm font-medium" id="copy-template-board-label">
        {m.board_title()}
      </p>
      <Select.Root type="single" bind:value={target} disabled={copying}>
        <Select.Trigger
          aria-labelledby="copy-template-board-label"
          class="w-full"
          >{targets.find((b) => String(b.id) === target)?.name ??
            m.ui_choose_a_board()}</Select.Trigger
        >
        <Select.Content>
          {#each targets as item (item.id)}
            <Select.Item value={String(item.id)} label={item.name}
              >{item.name}</Select.Item
            >
          {/each}
        </Select.Content>
      </Select.Root>
      {#if targets.length === 0}<p class="text-sm text-muted-foreground">
          {m.task_move_board_empty()}
        </p>{/if}
    </div>
    <Dialog.Footer>
      <Button
        variant="outline"
        disabled={copying}
        onclick={() => {
          open = false;
        }}>{m.common_cancel()}</Button
      >
      <Button disabled={copying || !valid} onclick={copy}
        >{copying ? m.template_copying() : m.template_copy_action()}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
