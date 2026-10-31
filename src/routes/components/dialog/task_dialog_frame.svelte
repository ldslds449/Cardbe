<script lang="ts">
  import type { Snippet } from "svelte";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import ResizeHandle from "../resize_handle.svelte";
  import {
    description_dialog_width_bounds,
    layout_widths,
    task_dialog_width_bounds,
  } from "../../layout-widths.svelte";

  // The resizable dialog shared by task details and the task editor, so
  // switching between them, or expanding the description, keeps one frame.
  let {
    open = $bindable(false),
    content_ref = $bindable(null),
    expanded = false,
    label,
    draft,
    draftVersion = 0,
    busy = false,
    onOpenFocus,
    onEscapeKeydown,
    onInteractOutside,
    onkeydown,
    children,
  }: {
    open: boolean;
    content_ref?: HTMLDivElement | null;
    /** Expanded descriptions use their own, wider width. */
    expanded?: boolean;
    label: string;
    draft?: unknown;
    draftVersion?: number;
    busy?: boolean;
    onOpenFocus: () => void;
    onEscapeKeydown?: (event: KeyboardEvent) => void;
    onInteractOutside?: () => void;
    onkeydown?: (event: KeyboardEvent) => void;
    children: Snippet;
  } = $props();

  // Expanding never narrows the dialog below its normal width.
  const expanded_bounds = $derived({
    ...description_dialog_width_bounds,
    min: Math.max(
      description_dialog_width_bounds.min,
      layout_widths.task_dialog_width,
    ),
    default: Math.max(
      description_dialog_width_bounds.default,
      layout_widths.task_dialog_width,
    ),
  });
  const width = $derived(
    expanded
      ? Math.max(
          layout_widths.description_dialog_width,
          layout_widths.task_dialog_width,
        )
      : layout_widths.task_dialog_width,
  );
</script>

<Dialog.Root bind:open {draft} {draftVersion} {busy}>
  <Dialog.Content
    bind:ref={content_ref}
    class="pb-0 outline-none sm:max-w-[calc(100%-2rem)]"
    style="width: {width}px"
    interactOutsideBehavior="close"
    {onkeydown}
    {onEscapeKeydown}
    {onInteractOutside}
    onOpenAutoFocus={(event) => {
      // The first focusable element is a width handle, so each body chooses
      // its own focus target.
      event.preventDefault();
      onOpenFocus();
    }}
  >
    {#each ["start", "end"] as const as edge (edge)}
      <ResizeHandle
        {width}
        bounds={expanded ? expanded_bounds : task_dialog_width_bounds}
        {label}
        {edge}
        scale={2}
        fit_viewport
        class={edge === "start" ? "-start-1.5" : "-end-1.5"}
        onresize={(next, commit) =>
          expanded
            ? layout_widths.set_description_dialog_width(next, commit)
            : layout_widths.set_task_dialog_width(next, commit)}
      />
    {/each}
    {@render children()}
  </Dialog.Content>
</Dialog.Root>
