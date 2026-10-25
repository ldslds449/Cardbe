<script lang="ts">
  import { untrack } from "svelte";
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as Select from "#lib/components/ui/select/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import XIcon from "@lucide/svelte/icons/x";
  import ChevronUpIcon from "@lucide/svelte/icons/chevron-up";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";

  import {
    MAX_SORT_RULES,
    SORT_DIRECTIONS,
    SORT_FIELDS,
    type SortDirection,
    type SortField,
    type SortRule,
  } from "../../type/column.svelte";
  import {
    DEFAULT_SORT_DIRECTION,
    active_sort_rules,
    sort_direction_label,
    sort_field_label,
  } from "../../utils/column-sort";

  // `key` keeps a row (and its focus) attached to its level when moved.
  type Level = { key: number; field: SortField; direction: SortDirection };

  let {
    open = $bindable(false),
    column_name,
    rules,
    onApply,
  }: {
    open?: boolean;
    column_name: string;
    rules: SortRule[];
    onApply: (rules: SortRule[]) => void;
  } = $props();

  let levels = $state<Level[]>([]);
  let next_key = 0;

  // Start each opening from the column's current rules.
  $effect.pre(() => {
    if (open) {
      levels = active_sort_rules(untrack(() => rules)).map(
        ({ field, direction }) => ({
          key: next_key++,
          field,
          direction,
        }),
      );
    }
  });

  function available_fields(index: number): SortField[] {
    return SORT_FIELDS.filter(
      (field) =>
        field === levels[index].field ||
        !levels.some((level) => level.field === field),
    );
  }

  function add_level() {
    const field = SORT_FIELDS.find(
      (candidate) => !levels.some((level) => level.field === candidate),
    );
    if (field && levels.length < MAX_SORT_RULES) {
      levels.push({
        key: next_key++,
        field,
        direction: DEFAULT_SORT_DIRECTION[field],
      });
    }
  }

  function move_level(index: number, offset: -1 | 1) {
    const [level] = levels.splice(index, 1);
    levels.splice(index + offset, 0, level);
  }

  function apply() {
    onApply(levels.map(({ field, direction }) => ({ field, direction })));
    open = false;
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-xl">
    <Dialog.Header>
      <Dialog.Title>
        {m.column_sort_dialog_title({ name: column_name })}
      </Dialog.Title>
      <Dialog.Description
        >{m.column_sort_dialog_description()}</Dialog.Description
      >
    </Dialog.Header>
    <div class="space-y-3">
      {#each levels as level, index (level.key)}
        {@const number = formatNumber(index + 1)}
        <div class="flex items-center gap-2">
          <span class="w-14 shrink-0 text-sm text-muted-foreground">
            {m.column_sort_level({ index: number })}
          </span>
          <Select.Root
            type="single"
            value={level.field}
            onValueChange={(value) => {
              const field = value as SortField;
              levels[index] = {
                key: level.key,
                field,
                direction: DEFAULT_SORT_DIRECTION[field],
              };
            }}
          >
            <Select.Trigger
              class="min-w-0 flex-1"
              aria-label={m.column_sort_level_field({ index: number })}
            >
              {sort_field_label(level.field)}
            </Select.Trigger>
            <Select.Content>
              {#each available_fields(index) as field (field)}
                <Select.Item value={field} label={sort_field_label(field)}>
                  {sort_field_label(field)}
                </Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <Select.Root
            type="single"
            value={level.direction}
            onValueChange={(value) => {
              level.direction = value as SortDirection;
            }}
          >
            <Select.Trigger
              class="w-36 shrink-0"
              aria-label={m.column_sort_level_direction({ index: number })}
            >
              {sort_direction_label(level.field, level.direction)}
            </Select.Trigger>
            <Select.Content>
              {#each SORT_DIRECTIONS as direction (direction)}
                <Select.Item
                  value={direction}
                  label={sort_direction_label(level.field, direction)}
                >
                  {sort_direction_label(level.field, direction)}
                </Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <div class="flex shrink-0">
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={m.column_sort_level_move_up({ index: number })}
              title={m.column_sort_level_move_up({ index: number })}
              disabled={index === 0}
              onclick={() => move_level(index, -1)}
            >
              <ChevronUpIcon />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={m.column_sort_level_move_down({ index: number })}
              title={m.column_sort_level_move_down({ index: number })}
              disabled={index === levels.length - 1}
              onclick={() => move_level(index, 1)}
            >
              <ChevronDownIcon />
            </Button>
          </div>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={m.column_sort_level_remove({ index: number })}
            title={m.column_sort_level_remove({ index: number })}
            onclick={() => levels.splice(index, 1)}
          >
            <XIcon />
          </Button>
        </div>
      {:else}
        <p class="text-sm text-muted-foreground">
          {m.column_sort_dialog_empty()}
        </p>
      {/each}
      <Button
        variant="outline"
        class="w-full border-dashed"
        disabled={levels.length >= MAX_SORT_RULES}
        onclick={add_level}
      >
        <PlusIcon />
        {m.column_sort_level_add()}
      </Button>
    </div>
    <Dialog.Footer>
      {#if active_sort_rules(rules).length > 0}
        <Button
          variant="outline"
          class="sm:mr-auto"
          onclick={() => {
            onApply([]);
            open = false;
          }}
        >
          <RotateCcwIcon />
          {m.column_sort_clear()}
        </Button>
      {/if}
      <Button
        variant="outline"
        onclick={() => {
          open = false;
        }}>{m.common_cancel()}</Button
      >
      <Button onclick={apply}>{m.column_sort_apply()}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
