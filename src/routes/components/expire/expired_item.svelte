<script lang="ts">
import * as Card from "$lib/components/ui/card/index.js";
import { Badge } from "$lib/components/ui/badge/index.js";
import { Button } from "$lib/components/ui/button/index.js";
import FullscreenIcon from "@lucide/svelte/icons/fullscreen";
import ClockIcon from "@lucide/svelte/icons/clock";

import type { Task } from "../../type/task.svelte";
import { display_task_color } from "../../utils/task-color";
import LabelBadge from "../label_badge.svelte";

interface ExpiredItemProps {
  task: Task;
  onShowDetail: (task: Task) => void;
}

let { task: t, onShowDetail }: ExpiredItemProps = $props();

function format_time(due_time: Date): string {
  const year = due_time.getFullYear();
  const month = String(due_time.getMonth() + 1).padStart(2, "0");
  const day = String(due_time.getDate()).padStart(2, "0");
  const hour = String(due_time.getHours()).padStart(2, "0");
  const minute = String(due_time.getMinutes()).padStart(2, "0");
  return `${year}/${month}/${day} ${hour}:${minute}`;
}
</script>

<Card.Root
  class="w-full border border-destructive/30 bg-destructive/5 py-2 transition-colors hover:border-destructive/50"
>
  <Card.Header class="py-2">
    <div class="flex w-full min-w-0 items-center gap-2">
      <div class="min-w-0 flex-1">
        <Card.Title
          class="truncate leading-tight text-sm"
          style="color: {display_task_color(t.color)};"
          title={t.title}
        >
          {t.title}
        </Card.Title>
        {#if t.due_time}
          <Card.Description class="mt-1">
            <Badge
              variant="outline"
              class="gap-1 border-destructive/30 bg-destructive/10 py-0 text-xs text-destructive"
            >
              <ClockIcon class="h-3 w-3" />
              {format_time(t.due_time)}
            </Badge>
          </Card.Description>
        {/if}
        {#if t.labels.length > 0}
          <div class="flex flex-wrap gap-1 mt-1">
            {#each t.labels as lbl (lbl)}
              <LabelBadge label={lbl} class="py-0 text-xs" />
            {/each}
          </div>
        {/if}
      </div>
      <Button
        size="icon"
        variant="outline"
        class="h-8 w-8 shrink-0"
        aria-label="View task details"
        title="View task details"
        onclick={() => {
                    onShowDetail(t);
                }}
      >
        <FullscreenIcon class="h-4 w-4" />
      </Button>
    </div>
  </Card.Header>
</Card.Root>
