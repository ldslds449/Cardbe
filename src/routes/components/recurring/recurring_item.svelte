<script lang="ts">
  import * as Card from "$lib/components/ui/card/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";

  import ClockIcon from "@lucide/svelte/icons/clock";
  import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import Repeat2Icon from "@lucide/svelte/icons/repeat-2";
  import XIcon from "@lucide/svelte/icons/x";

  import type { Column } from "../../type/column.svelte";
  import { recurrence_label, type Task } from "../../type/task.svelte";
  import { display_task_color } from "../../utils/task-color";

  let {
    column,
    task,
    onView,
    onEdit,
    onStopRepeat,
  }: {
    column: Column;
    task: Task;
    onView: (task: Task) => void;
    onEdit: (task: Task) => void;
    onStopRepeat: (task: Task) => void;
  } = $props();

  function format_due_time(date: Date | undefined): string {
    if (!date) return "No due time";
    return date.toLocaleString([], {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

<Card.Root class="w-full gap-2 border-2 py-3">
  <Card.Header class="px-4">
    <div class="flex min-w-0 items-start gap-2">
      <div class="min-w-0 flex-1">
        <Card.Title
          class="truncate text-sm"
          style="color: {display_task_color(task.color)};"
          title={task.title}
        >
          {task.title}
        </Card.Title>
        <div class="mt-2 flex flex-wrap gap-1.5">
          <Badge variant="outline" class="max-w-full truncate font-normal">
            {column.name}
          </Badge>
          <Badge variant="secondary" class="gap-1 font-normal">
            <Repeat2Icon class="size-3" />
            {recurrence_label(task.recurrence)}
          </Badge>
        </div>
        <p class="mt-2 flex items-center gap-1 text-xs text-muted-foreground">
          <ClockIcon class="size-3.5" />
          {format_due_time(task.due_time)}
        </p>
      </div>
      <div class="flex shrink-0 gap-1">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`View ${task.title}`}
          title="View details"
          onclick={() => onView(task)}
        >
          <FullscreenIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`Edit ${task.title}`}
          title="Edit task"
          onclick={() => onEdit(task)}
        >
          <PencilIcon />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          class="text-muted-foreground hover:text-destructive"
          aria-label={`Stop repeating ${task.title}`}
          title="Stop repeating"
          onclick={() => onStopRepeat(task)}
        >
          <XIcon />
        </Button>
      </div>
    </div>
  </Card.Header>
</Card.Root>
