<script lang="ts">
import { useDroppable } from "@dnd-kit-svelte/svelte";
import type { Snippet } from "svelte";

interface CalendarDayDropZoneProps {
  day_key: string;
  class?: string;
  title?: string;
  ondblclick?: (event: MouseEvent) => void;
  disabled?: boolean;
  children: Snippet;
}

let {
  day_key,
  class: class_name = "",
  title,
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
  class={`${class_name} ${isDropTarget.current ? "bg-primary/10 ring-2 ring-inset ring-primary" : ""}`}
  {title}
  {ondblclick}
>
  {@render children()}
</div>
