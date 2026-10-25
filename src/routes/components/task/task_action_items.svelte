<script lang="ts">
  import type { Snippet } from "svelte";
  import * as ContextMenu from "#lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import * as m from "#lib/paraglide/messages.js";
  import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import ArchiveIcon from "@lucide/svelte/icons/archive";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import TrashIcon from "@lucide/svelte/icons/trash-2";

  let {
    context = false,
    onView,
    onEdit,
    onComplete,
    onArchive,
    onDelete,
    extra,
    afterArchive,
  }: {
    context?: boolean;
    onView: () => void;
    onEdit?: () => void;
    /** Moves the task to the done column, or archives it without one. */
    onComplete?: () => void;
    onArchive?: () => void;
    onDelete?: () => void;
    extra?: Snippet;
    afterArchive?: Snippet;
  } = $props();
  const Item = $derived(context ? ContextMenu.Item : DropdownMenu.Item);
  const Separator = $derived(
    context ? ContextMenu.Separator : DropdownMenu.Separator,
  );
</script>

<Item onclick={onView}><FullscreenIcon />{m.task_view_details()}</Item>
{#if onEdit}<Item onclick={onEdit}><PencilIcon />{m.task_edit_action()}</Item
  >{/if}
{#if onComplete}<Item onSelect={onComplete}
    ><CircleCheckIcon />{m.task_complete()}</Item
  >{/if}
{@render extra?.()}
{#if onArchive}<Item onSelect={onArchive}
    ><ArchiveIcon />{m.task_archive_action()}</Item
  >{/if}
{@render afterArchive?.()}
{#if onDelete}
  <Separator />
  <Item variant="destructive" onclick={onDelete}
    ><TrashIcon />{m.task_delete()}</Item
  >
{/if}
