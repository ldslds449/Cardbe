<script lang="ts">
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
      <AlertDialog.Title>Delete Task</AlertDialog.Title>
      <AlertDialog.Description>
        Are you sure you want to delete “{task_title ?? "this task"}”?
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
      <AlertDialog.Action onclick={confirm_delete}>Confirm</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
