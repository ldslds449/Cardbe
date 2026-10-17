<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";

  let {
    notice,
    onacknowledge,
  }: {
    notice: {
      board_name: string;
      reason: "SHARE_ACCESS_TERMINATED" | "SHARE_INVITATION_DELETED";
      switched_to: string | null;
    };
    onacknowledge: () => void;
  } = $props();
</script>

<AlertDialog.Root open={true}>
  <AlertDialog.Content
    class="max-h-[calc(100dvh-2rem)] overflow-y-auto"
    onEscapeKeydown={(event) => event.preventDefault()}
    onInteractOutside={(event) => event.preventDefault()}
  >
    <AlertDialog.Header>
      <AlertDialog.Title>{m.share_removed_notice_title()}</AlertDialog.Title>
      <AlertDialog.Description class="break-words">
        {notice.reason === "SHARE_INVITATION_DELETED"
          ? m.share_removed_notice_invitation({ name: notice.board_name })
          : m.share_removed_notice_revoked({ name: notice.board_name })}
        {#if notice.switched_to !== null}
          <span class="mt-2 block"
            >{m.share_removed_notice_switched({
              name: notice.switched_to,
            })}</span
          >
        {/if}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Action onclick={onacknowledge}>
        {m.share_removed_notice_acknowledge()}
      </AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
