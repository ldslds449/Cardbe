<script lang="ts">
  import { onMount } from "svelte";
  import { beforeNavigate, goto } from "$app/navigation";
  import * as AlertDialog from "$lib/components/ui/alert-dialog";
  import * as m from "$lib/paraglide/messages.js";
  import {
    confirmUnsavedChanges,
    hasUnsavedChanges,
    setUnsavedChangesPrompt,
  } from "$lib/unsaved-changes";

  let open = $state(false);
  let resolve: ((discard: boolean) => void) | undefined;
  let pending: Promise<boolean> | undefined;
  let navigating = false;

  function finish(discard: boolean) {
    open = false;
    resolve?.(discard);
    resolve = undefined;
    pending = undefined;
  }

  onMount(() => {
    setUnsavedChangesPrompt(() => {
      pending ??= new Promise<boolean>((done) => {
        resolve = done;
        open = true;
      });
      return pending;
    });
    return () => {
      finish(false);
      setUnsavedChangesPrompt();
    };
  });

  beforeNavigate((navigation) => {
    if (navigating || !hasUnsavedChanges() || navigation.willUnload) {
      return;
    }
    navigation.cancel();
    void confirmUnsavedChanges().then(async (discard) => {
      if (discard && navigation.to) {
        navigating = true;
        try {
          await goto(navigation.to.url);
        } finally {
          navigating = false;
        }
      }
    });
  });

  function beforeUnload(event: BeforeUnloadEvent) {
    if (hasUnsavedChanges()) {
      event.preventDefault();
      event.returnValue = "";
    }
  }
</script>

<svelte:window onbeforeunload={beforeUnload} />
<AlertDialog.Root
  {open}
  onOpenChange={(value) => {
    if (!value) {
      finish(false);
    }
  }}
>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{m.unsaved_changes_title()}</AlertDialog.Title>
      <AlertDialog.Description
        >{m.unsaved_changes_description()}</AlertDialog.Description
      >
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel onclick={() => finish(false)}
        >{m.unsaved_changes_keep_editing()}</AlertDialog.Cancel
      >
      <AlertDialog.Action variant="destructive" onclick={() => finish(true)}
        >{m.unsaved_changes_discard()}</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
