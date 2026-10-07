<script lang="ts">
  import { Dialog as DialogPrimitive } from "bits-ui";
  import { onMount, untrack } from "svelte";
  import {
    confirmUnsavedChanges,
    registerUnsavedChanges,
  } from "$lib/unsaved-changes";

  let {
    open = $bindable(false),
    draft,
    draftVersion = 0,
    busy = false,
    ...restProps
  }: DialogPrimitive.RootProps & {
    draft?: unknown;
    draftVersion?: number;
    busy?: boolean;
  } = $props();
  let initial = "";
  $effect(() => {
    const baseline = { open, draftVersion };
    if (baseline.open) {
      initial = untrack(() => JSON.stringify(draft));
    }
  });
  const isDirty = () =>
    open && draft !== undefined && JSON.stringify(draft) !== initial;
  onMount(() => registerUnsavedChanges(isDirty));

  function setOpen(value: boolean) {
    if (value) {
      open = true;
      return;
    }
    if (busy) {
      return;
    }
    if (!isDirty()) {
      open = false;
      return;
    }
    void confirmUnsavedChanges(isDirty).then((discard) => {
      if (discard) {
        open = false;
      }
    });
  }
</script>

<DialogPrimitive.Root bind:open={() => open, setOpen} {...restProps} />
