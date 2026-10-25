<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import * as Field from "#lib/components/ui/field/index.js";
  import * as Popover from "#lib/components/ui/popover/index.js";
  import * as Command from "#lib/components/ui/command/index.js";
  import { Button } from "#lib/components/ui/button/index.js";

  import PlusIcon from "@lucide/svelte/icons/plus";
  import XIcon from "@lucide/svelte/icons/x";
  import CircleHelpIcon from "@lucide/svelte/icons/circle-help";

  import type { Column } from "../../type/column.svelte";
  import { MAX_BLOCKERS, can_wait_on } from "../../utils/blocking";

  // Chooses the tasks of the same board that a task has to wait for.
  let {
    task_id,
    blocked_by = $bindable(undefined),
    columns,
  }: {
    task_id: string;
    blocked_by?: string[];
    columns: Column[];
  } = $props();

  let open = $state(false);
  let chosen = $derived(blocked_by ?? []);
  let options = $derived(
    columns.flatMap((column) =>
      column.tasks.map((task) => ({ task, column_name: column.name })),
    ),
  );
  // References to archived or removed tasks stay stored but are not listed.
  let selected = $derived(
    options.filter((option) => chosen.includes(option.task.id)),
  );
  let candidates = $derived(
    options.filter((option) =>
      can_wait_on(task_id, chosen, option.task.id, columns),
    ),
  );

  function add(id: string) {
    blocked_by = [...chosen, id];
    open = false;
  }

  function remove(id: string) {
    const next = chosen.filter((candidate) => candidate !== id);
    blocked_by = next.length > 0 ? next : undefined;
  }
</script>

<Field.Field>
  <div class="flex items-center justify-between gap-2">
    <div class="flex items-center gap-1.5">
      <Field.FieldLabel id="task-blockers-label"
        >{m.task_blockers()}</Field.FieldLabel
      >
      <div class="group/blockers-help relative inline-flex">
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          class="text-muted-foreground"
          aria-label={m.task_blockers_help()}
          aria-describedby="task-blockers-help"
        >
          <CircleHelpIcon class="size-3.5" />
        </Button>
        <div
          id="task-blockers-help"
          role="tooltip"
          class="pointer-events-none absolute bottom-full left-0 z-50 mb-2 w-max max-w-64 rounded-md bg-popover px-3 py-2 text-xs leading-relaxed text-popover-foreground opacity-0 shadow-md ring-1 ring-foreground/10 transition-opacity group-hover/blockers-help:opacity-100 group-focus-within/blockers-help:opacity-100"
        >
          {m.task_blockers_description()}
        </div>
      </div>
    </div>
    <Popover.Root bind:open>
      <Popover.Trigger disabled={chosen.length >= MAX_BLOCKERS}>
        {#snippet child({ props })}
          <Button
            {...props}
            type="button"
            variant="ghost"
            size="sm"
            class="h-7 gap-1.5 px-2 text-xs"
          >
            <PlusIcon class="size-3.5" />
            {m.task_blockers_add()}
          </Button>
        {/snippet}
      </Popover.Trigger>
      <Popover.Content class="w-80 p-0" align="end">
        <Command.Root>
          <Command.Input
            placeholder={m.task_search_cards_placeholder()}
            aria-label={m.task_search_cards()}
          />
          <Command.List class="max-h-64">
            <Command.Empty>{m.editor_no_cards()}</Command.Empty>
            <Command.Group value="cards">
              {#each candidates as option (option.task.id)}
                <Command.Item
                  value={option.task.id}
                  keywords={[option.task.title, option.column_name]}
                  onSelect={() => add(option.task.id)}
                >
                  <div class="min-w-0">
                    <div class="truncate">
                      {option.task.title || m.task_untitled()}
                    </div>
                    <div class="truncate text-xs text-muted-foreground">
                      {option.column_name}
                    </div>
                  </div>
                </Command.Item>
              {/each}
            </Command.Group>
          </Command.List>
        </Command.Root>
      </Popover.Content>
    </Popover.Root>
  </div>
  {#if selected.length > 0}
    <ul class="space-y-1" aria-labelledby="task-blockers-label">
      {#each selected as option (option.task.id)}
        <li class="flex items-center gap-2 rounded-md border px-2 py-1 text-sm">
          <div class="min-w-0 flex-1">
            <div class="truncate">{option.task.title || m.task_untitled()}</div>
            <div class="truncate text-xs text-muted-foreground">
              {option.column_name}
            </div>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            class="text-muted-foreground"
            aria-label={m.task_blockers_remove({
              name: option.task.title || m.task_untitled(),
            })}
            onclick={() => remove(option.task.id)}
          >
            <XIcon class="size-3.5" />
          </Button>
        </li>
      {/each}
    </ul>
  {/if}
</Field.Field>
