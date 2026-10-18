<script lang="ts">
  import { useDroppable } from "@dnd-kit-svelte/svelte";
  import type { Snippet } from "svelte";

  interface CalendarDayDropZoneProps {
    day_key: string;
    class?: string;
    style?: string;
    title?: string;
    /** The day is inside the range a drag would produce. */
    preview?: "valid" | "invalid" | null;
    ondblclick?: (event: MouseEvent) => void;
    disabled?: boolean;
    children: Snippet;
  }

  let {
    day_key,
    class: class_name = "",
    style,
    title,
    preview = null,
    ondblclick,
    disabled = false,
    children,
  }: CalendarDayDropZoneProps = $props();

  const { ref, isDropTarget } = useDroppable({
    id: () => `calendar-day:${day_key}`,
    type: "calendar-day",
    accept: ["calendar-task"],
    disabled: () => disabled,
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  {@attach ref}
  class={`${class_name} ${
    preview === "invalid"
      ? "bg-destructive/10 ring-2 ring-inset ring-destructive"
      : isDropTarget.current
        ? "bg-primary/10 ring-2 ring-inset ring-primary"
        : preview === "valid"
          ? "bg-primary/5 ring-1 ring-inset ring-primary/50"
          : ""
  }`}
  {style}
  {title}
  {ondblclick}
>
  {@render children()}
</div>
