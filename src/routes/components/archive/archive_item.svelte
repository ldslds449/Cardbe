<script lang="ts">
import * as Card from "$lib/components/ui/card/index.js";
import UndoIcon from "@lucide/svelte/icons/undo-2";
import FullscreenIcon from "@lucide/svelte/icons/fullscreen";

import type { Task } from "../../type/task.svelte";
import { display_task_color } from "../../utils/task-color";
import { Button } from "$lib/components/ui/button/index.js";
import LabelBadge from "../label_badge.svelte";

interface ArchiveItemProps {
  date: string;
  tasks: Task[];
  onUnarchive: (task_id: string) => void;
  onShowDetail: (task: Task) => void;
}

let { date, tasks, onUnarchive, onShowDetail }: ArchiveItemProps = $props();
</script>

<div>
  <div class="flex justify-center p-4">
    <h3 class="scroll-m-20 text-xl font-semibold tracking-tight">
      {date}
    </h3>
  </div>
  <div class="space-y-2 p-2">
    {#each tasks as t (t.id)}
      <Card.Root
        class="w-full border bg-muted/20 py-1 transition-colors hover:border-muted-foreground/40"
      >
        <Card.Header class="py-2">
          <div class="flex w-full min-w-0 items-center gap-2 h-full">
            <div class="min-w-0 flex-1 self-center">
              <Card.Title
                class="truncate leading-tight text-sm"
                style="color: {display_task_color(t.color)};"
                title={t.title}
              >
                {t.title}
              </Card.Title>
              {#if t.labels.length > 0}
                <div class="flex flex-wrap gap-1 mt-1">
                  {#each t.labels as lbl (lbl)}
                    <LabelBadge label={lbl} class="py-0 text-xs" />
                  {/each}
                </div>
              {/if}
            </div>
            <div class="flex gap-1 shrink-0">
              <Button
                size="icon"
                variant="outline"
                class="h-8 w-8"
                aria-label="View task details"
                title="View task details"
                onclick={() => {
                                    onShowDetail(t);
                                }}
              >
                <FullscreenIcon class="h-4 w-4" />
              </Button>
              <Button
                size="icon"
                variant="outline"
                class="h-8 w-8 text-muted-foreground hover:border-primary/50 hover:text-primary"
                aria-label="Unarchive task"
                title="Unarchive task"
                onclick={() => {
                                    onUnarchive(t.id);
                                }}
              >
                <UndoIcon class="text-white" />
              </Button>
            </div>
          </div>
        </Card.Header>
      </Card.Root>
    {/each}
  </div>
</div>
