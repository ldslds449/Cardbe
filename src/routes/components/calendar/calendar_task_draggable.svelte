<script lang="ts">
  import { useDraggable } from "@dnd-kit-svelte/svelte";
  import type { Snippet } from "svelte";

  interface CalendarTaskDraggableProps {
    task_id: string;
    instance_key: string;
    disabled?: boolean;
    class?: string;
    oncontextmenu?: (event: MouseEvent) => void;
    children: Snippet;
  }

  let {
    task_id,
    instance_key,
    disabled = false,
    class: class_name = "",
    oncontextmenu,
    children,
  }: CalendarTaskDraggableProps = $props();

  const { ref, isDragging } = useDraggable({
    id: () => `calendar-task:${task_id}:${instance_key}`,
    type: "calendar-task",
    disabled: () => disabled,
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  {@attach ref}
  class={`${class_name} ${isDragging.current ? "opacity-40" : ""}`}
  {oncontextmenu}
>
  {@render children()}
</div>
