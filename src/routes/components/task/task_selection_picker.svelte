<script lang="ts">
  import { Checkbox } from "#lib/components/ui/checkbox/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import { buttonVariants } from "#lib/components/ui/button/index.js";
  import SearchIcon from "@lucide/svelte/icons/search";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import { formatNumber } from "#lib/i18n/index.js";
  import * as m from "#lib/paraglide/messages.js";
  import type { Column } from "../../type/column.svelte";
  import {
    filter_selection_columns,
    select_column_tasks,
    select_one_task,
    select_visible_tasks,
    type TaskSelection,
  } from "./task_selection";
  let {
    columns,
    selected_column_ids = $bindable([]),
    selected_task_ids = $bindable([]),
    query = $bindable(""),
    disabled = false,
  }: {
    columns: Column[];
    selected_column_ids?: string[];
    selected_task_ids?: string[];
    query?: string;
    disabled?: boolean;
  } = $props();
  let filter_column_ids = $state<string[]>([]);
  // Ignore filtered columns that were removed from the board meanwhile.
  const active_filter_ids = $derived(
    filter_column_ids.filter((id) =>
      columns.some((column) => column.id === id),
    ),
  );
  const filter_label = $derived(
    active_filter_ids.length === 0
      ? m.explorer_all_columns()
      : active_filter_ids.length === 1
        ? (columns.find((column) => column.id === active_filter_ids[0])?.name ??
          m.explorer_all_columns())
        : m.task_picker_filtered_columns({ count: active_filter_ids.length }),
  );
  const visible_columns = $derived(
    filter_selection_columns(columns, query, active_filter_ids),
  );
  const visible_ids = $derived(
    visible_columns.flatMap((column) => column.tasks.map((task) => task.id)),
  );
  const selection = $derived({
    column_ids: selected_column_ids,
    task_ids: selected_task_ids,
  });
  function apply(next: TaskSelection) {
    selected_column_ids = next.column_ids;
    selected_task_ids = next.task_ids;
  }
</script>

<div class="flex min-h-0 flex-col gap-2">
  <div class="flex shrink-0 gap-2">
    <div class="relative min-w-0 flex-1">
      <SearchIcon
        class="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground"
      />
      <Input
        class="pl-9"
        bind:value={query}
        placeholder={m.ui_search_columns_tasks_descriptions_or_labels()}
        aria-label={m.ui_search()}
      />
    </div>
    <DropdownMenu.Root>
      <DropdownMenu.Trigger
        class={buttonVariants({
          variant: "outline",
          class: "max-w-40 shrink-0 justify-between font-normal",
        })}
        disabled={disabled || columns.length === 0}
        aria-label={m.task_picker_column_filter()}
        title={m.task_picker_column_filter()}
      >
        <span class="min-w-0 truncate">{filter_label}</span>
        <ChevronDownIcon />
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="max-h-72 w-56">
        <DropdownMenu.CheckboxItem
          checked={active_filter_ids.length === 0}
          closeOnSelect={false}
          onCheckedChange={() => (filter_column_ids = [])}
          >{m.explorer_all_columns()}</DropdownMenu.CheckboxItem
        >
        <DropdownMenu.Separator />
        {#each columns as column (column.id)}
          <DropdownMenu.CheckboxItem
            checked={active_filter_ids.includes(column.id)}
            closeOnSelect={false}
            onCheckedChange={(checked) =>
              (filter_column_ids = checked
                ? [...active_filter_ids, column.id]
                : active_filter_ids.filter((id) => id !== column.id))}
            ><span class="truncate">{column.name}</span
            ></DropdownMenu.CheckboxItem
          >
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </div>
  <div class="flex shrink-0 items-center justify-between gap-3">
    <span class="text-xs tabular-nums text-muted-foreground"
      >{query.trim()
        ? m.share_matching_tasks({ count: visible_ids.length })
        : m.share_available_tasks({ count: visible_ids.length })}</span
    >
    <div class="flex gap-1">
      <Button
        variant="ghost"
        size="sm"
        disabled={disabled ||
          !visible_ids.length ||
          visible_ids.every((id) => selected_task_ids.includes(id))}
        onclick={() =>
          apply(
            select_visible_tasks(columns, visible_columns, selection, true),
          )}>{m.ui_select_all()}</Button
      >
      <Button
        variant="ghost"
        size="sm"
        disabled={disabled ||
          visible_ids.every((id) => !selected_task_ids.includes(id))}
        onclick={() =>
          apply(
            select_visible_tasks(columns, visible_columns, selection, false),
          )}>{m.ui_clear_all()}</Button
      >
    </div>
  </div>
  <ScrollArea
    class="h-72 min-h-0 shrink rounded-md border"
    role="group"
    aria-label={m.ui_content_to_include()}
  >
    <div class="space-y-2 p-2">
      {#each visible_columns as column (column.id)}
        {@const all_ids =
          columns
            .find((candidate) => candidate.id === column.id)
            ?.tasks.map((task) => task.id) ?? []}
        {@const selected_count = all_ids.filter((id) =>
          selected_task_ids.includes(id),
        ).length}
        <div class="rounded-md border bg-muted/20">
          <label
            class="flex cursor-pointer items-center gap-3 rounded-t-md px-3 py-2 hover:bg-muted/60"
          >
            <Checkbox
              {disabled}
              checked={selected_column_ids.includes(column.id) &&
                selected_count === all_ids.length}
              indeterminate={selected_column_ids.includes(column.id) &&
                all_ids.length > 0 &&
                selected_count < all_ids.length}
              onCheckedChange={(checked) =>
                apply(
                  select_column_tasks(
                    columns,
                    selection,
                    column.id,
                    checked === true,
                  ),
                )}
            />
            <span class="min-w-0 flex-1 truncate text-sm font-medium"
              >{column.name}</span
            >
            <span class="text-xs tabular-nums text-muted-foreground"
              >{formatNumber(selected_count)}/{formatNumber(
                all_ids.length,
              )}</span
            >
          </label>
          {#if column.tasks.length > 0}
            <div class="border-t px-2 py-1">
              {#each column.tasks as task (task.id)}
                <label
                  class="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 hover:bg-muted/60"
                >
                  <Checkbox
                    class="mt-0.5"
                    {disabled}
                    checked={selected_task_ids.includes(task.id)}
                    onCheckedChange={(checked) =>
                      apply(
                        select_one_task(
                          selection,
                          column.id,
                          task.id,
                          checked === true,
                        ),
                      )}
                  />
                  <span class="min-w-0 flex-1">
                    <span class="block wrap-anywhere text-sm"
                      >{task.title || m.task_untitled()}</span
                    >
                    {#if task.labels.length > 0}<span
                        class="block wrap-anywhere text-xs text-muted-foreground"
                        >{task.labels.join(" · ")}</span
                      >{/if}
                  </span>
                </label>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <p class="p-5 text-center text-sm text-muted-foreground">
          {m.ui_no_matching_columns_or_tasks()}
        </p>
      {/each}
    </div>
  </ScrollArea>
</div>
