<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as Empty from "#lib/components/ui/empty/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";

  import PencilIcon from "@lucide/svelte/icons/pencil";
  import PictureInPicture2Icon from "@lucide/svelte/icons/picture-in-picture-2";
  import SearchXIcon from "@lucide/svelte/icons/search-x";

  import type { AllTaskItem } from "../../board-base.svelte";
  import type { Focus } from "../../type/focus";
  import TaskResultList from "../task/task_result_list.svelte";
  import type { TaskRowBoard } from "../task/task_row.svelte";
  import { load_focus_results } from "./focus-results";

  let {
    view,
    boards,
    paused = false,
    onEdit,
    onFloat,
    onOpenTask,
    onEditTask,
    onArchiveTask,
    onOpenBoard,
  }: {
    view: Focus;
    boards: TaskRowBoard[];
    /** Set while a dialog opened from this page may still change tasks. */
    paused?: boolean;
    onEdit: () => void;
    onFloat?: () => void;
    onOpenTask: (item: AllTaskItem) => unknown;
    onEditTask: (item: AllTaskItem) => unknown;
    onArchiveTask: (item: AllTaskItem) => unknown;
    onOpenBoard: (board_id: number) => void;
  } = $props();
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  <header class="flex shrink-0 items-center gap-2 border-b px-5 py-3">
    <h2 class="min-w-0 flex-1 truncate text-base font-semibold">
      {view.name}
    </h2>
    {#if onFloat}
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={m.focus_float_on_desktop()}
        title={m.focus_float_on_desktop()}
        onclick={onFloat}
      >
        <PictureInPicture2Icon />
      </Button>
    {/if}
    <Button variant="outline" size="sm" onclick={onEdit}>
      <PencilIcon />
      {m.common_edit()}
    </Button>
  </header>

  <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="vertical">
    <div class="mx-auto w-full max-w-4xl p-5">
      <TaskResultList
        load_key={JSON.stringify(view)}
        load={(now, cursor, limit) =>
          load_focus_results(view, now, cursor, limit)}
        group={view.group}
        label={view.name}
        {boards}
        {paused}
        highlight={view.filter.query}
        {onOpenTask}
        {onEditTask}
        {onArchiveTask}
        {onOpenBoard}
      >
        {#snippet empty()}
          <Empty.Root>
            <Empty.Header>
              <Empty.Media variant="icon"><SearchXIcon /></Empty.Media>
              <Empty.Title>{m.focus_empty()}</Empty.Title>
              <Empty.Description
                >{m.focus_empty_description()}</Empty.Description
              >
            </Empty.Header>
            <Empty.Content>
              <Button variant="outline" onclick={onEdit}>
                <PencilIcon />
                {m.common_edit()}
              </Button>
            </Empty.Content>
          </Empty.Root>
        {/snippet}
      </TaskResultList>
    </div>
  </ScrollArea>
</div>
