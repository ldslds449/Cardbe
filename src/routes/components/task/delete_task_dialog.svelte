<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";

  interface DeleteTaskDialogProps {
    open: boolean;
    task_title?: string;
    onConfirm?: () => void;
  }

  let {
    open = $bindable(false),
    task_title,
    onConfirm = () => {},
  }: DeleteTaskDialogProps = $props();

  function confirm_delete() {
    open = false;
    onConfirm();
  }
</script>

<AlertDialog.Root bind:open>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{m.task_delete()}</AlertDialog.Title>
      <AlertDialog.Description>
        {task_title
          ? m.task_delete_named_confirm({ name: task_title })
          : m.task_delete_confirm()}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{m.common_cancel()}</AlertDialog.Cancel>
      <AlertDialog.Action onclick={confirm_delete}
        >{m.common_confirm()}</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
