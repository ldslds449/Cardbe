<script lang="ts">
  import { m } from "$lib/paraglide/messages.js";
  import "$lib/i18n/locale.svelte";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Field from "$lib/components/ui/field/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { TagInput } from "$lib/components/ui/tag-input/index.js";

  import {
    clone_smart_view,
    create_smart_view,
    MAX_SMART_VIEW_NAME_LENGTH,
    type SmartView,
    type SmartViewDue,
    type SmartViewGroup,
    type SmartViewSort,
  } from "../../type/smart-view";
  import { priority_label, TASK_PRIORITIES } from "../../type/task.svelte";
  import type {
    CompletionFilter,
    PriorityFilter,
    WaitingFilter,
  } from "../../utils/task-explorer";
  import MultiSelect from "./multi_select.svelte";

  let {
    open = $bindable(false),
    view,
    boards,
    label_suggestions,
    onSave,
  }: {
    open: boolean;
    /** The view to edit, or null to create one. */
    view: SmartView | null;
    boards: { id: number; name: string }[];
    label_suggestions: string[];
    onSave: (view: SmartView) => Promise<boolean>;
  } = $props();

  let draft = $state<SmartView>(create_smart_view(""));
  let invalid_name = $state(false);
  let saving = $state(false);

  $effect(() => {
    if (open) {
      draft = view ? clone_smart_view(view) : create_smart_view("");
      invalid_name = false;
    }
  });

  // Only boards that still exist; none chosen covers every board.
  let board_ids = $derived(
    (draft.filter.board_ids ?? []).filter((id) =>
      boards.some((board) => board.id === id),
    ),
  );

  const completion_options = $derived<
    { value: CompletionFilter; label: string }[]
  >([
    { value: "open", label: m.smart_view_completion_open() },
    { value: "done", label: m.smart_view_completion_done() },
    { value: "archived", label: m.smart_view_completion_archived() },
  ]);
  const priority_options = $derived<{ value: PriorityFilter; label: string }[]>(
    [
      ...[...TASK_PRIORITIES].reverse().map((priority) => ({
        value: priority,
        label: priority_label(priority),
      })),
      { value: "none", label: m.task_priority_none() },
    ],
  );
  const due_options = $derived<{ value: SmartViewDue; label: string }[]>([
    { value: "all", label: m.smart_view_due_all() },
    { value: "overdue", label: m.smart_view_due_overdue() },
    { value: "today", label: m.smart_view_due_today() },
    { value: "by_today", label: m.smart_view_due_by_today() },
    { value: "next_7_days", label: m.smart_view_due_next_7_days() },
    { value: "none", label: m.smart_view_due_none() },
  ]);
  const waiting_options = $derived<
    { value: WaitingFilter | "any"; label: string }[]
  >([
    { value: "any", label: m.smart_view_any() },
    { value: "waiting", label: m.smart_view_waiting_waiting() },
    { value: "not_waiting", label: m.smart_view_waiting_not_waiting() },
    { value: "waited_on", label: m.smart_view_waiting_waited_on() },
  ]);
  const recurring_options = $derived([
    { value: "any", label: m.smart_view_any() },
    { value: "only", label: m.smart_view_recurring_only() },
    { value: "exclude", label: m.smart_view_recurring_exclude() },
  ]);
  const sort_options = $derived<{ value: SmartViewSort; label: string }[]>([
    { value: "priority", label: m.smart_view_sort_priority() },
    { value: "due", label: m.smart_view_sort_due() },
    { value: "created", label: m.smart_view_sort_created() },
    { value: "title", label: m.smart_view_sort_title() },
  ]);
  const group_options = $derived<{ value: SmartViewGroup; label: string }[]>([
    { value: "none", label: m.smart_view_group_none() },
    { value: "board", label: m.smart_view_group_board() },
    { value: "priority", label: m.smart_view_group_priority() },
    { value: "due", label: m.smart_view_group_due() },
  ]);

  function label_of<T>(options: { value: T; label: string }[], value: T) {
    return options.find((option) => option.value === value)?.label ?? "";
  }

  let recurring_value = $derived(
    draft.filter.recurring === null
      ? "any"
      : draft.filter.recurring
        ? "only"
        : "exclude",
  );

  async function save() {
    const name = draft.name.trim();
    if (!name) {
      invalid_name = true;
      return;
    }
    if (saving) {
      return;
    }
    saving = true;
    try {
      const saved: SmartView = {
        ...clone_smart_view(draft),
        name,
      };
      saved.filter.query = saved.filter.query.trim();
      saved.filter.board_ids = board_ids.length > 0 ? board_ids : null;
      if (await onSave(saved)) {
        open = false;
      }
    } finally {
      saving = false;
    }
  }
</script>

{#snippet single_select<T extends string>(
  id: string,
  label: string,
  options: { value: T; label: string }[],
  value: T,
  onChange: (value: T) => void,
)}
  <Field.Field class="min-w-0">
    <Field.FieldLabel id="{id}-label">{label}</Field.FieldLabel>
    <Select.Root
      type="single"
      {value}
      onValueChange={(next) => onChange(next as T)}
    >
      <Select.Trigger class="w-full min-w-0" aria-labelledby="{id}-label">
        <span class="truncate">{label_of(options, value)}</span>
      </Select.Trigger>
      <Select.Content>
        {#each options as option (option.value)}
          <Select.Item value={option.value}>{option.label}</Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>
  </Field.Field>
{/snippet}

<Dialog.Root bind:open busy={saving}>
  <Dialog.Content
    class="max-h-[85dvh] grid-rows-[auto_minmax(0,1fr)] gap-0 overflow-hidden p-0 sm:max-w-lg"
  >
    <Dialog.Header class="border-b p-6 pb-4">
      <Dialog.Title
        >{view ? m.smart_view_edit_title() : m.smart_view_new()}</Dialog.Title
      >
      <Dialog.Description
        >{m.smart_view_dialog_description()}</Dialog.Description
      >
    </Dialog.Header>
    <form
      class="grid min-h-0 grid-rows-[minmax(0,1fr)_auto]"
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <ScrollArea class="min-h-0" orientation="vertical">
        <div class="grid gap-4 p-6">
          <Field.Field data-invalid={invalid_name}>
            <Field.FieldLabel for="smart-view-name"
              >{m.smart_view_name()}</Field.FieldLabel
            >
            <Input
              id="smart-view-name"
              bind:value={draft.name}
              maxlength={MAX_SMART_VIEW_NAME_LENGTH}
              aria-invalid={invalid_name}
              oninput={() => (invalid_name = false)}
              autofocus
            />
            {#if invalid_name}
              <Field.Error>{m.smart_view_name_required()}</Field.Error>
            {/if}
          </Field.Field>
          <Field.Field>
            <Field.FieldLabel for="smart-view-query"
              >{m.smart_view_query()}</Field.FieldLabel
            >
            <Input
              id="smart-view-query"
              bind:value={draft.filter.query}
              placeholder={m.smart_view_query_placeholder()}
              maxlength={500}
            />
          </Field.Field>
          <div class="grid gap-4 sm:grid-cols-2">
            <Field.Field class="min-w-0">
              <Field.FieldLabel id="smart-view-boards-label"
                >{m.smart_view_boards()}</Field.FieldLabel
              >
              <MultiSelect
                id="smart-view-boards"
                label_id="smart-view-boards-label"
                options={boards.map((board) => ({
                  value: board.id,
                  label: board.name,
                }))}
                bind:selected={
                  () => board_ids,
                  (ids) =>
                    (draft.filter.board_ids = ids.length > 0 ? ids : null)
                }
                empty_label={m.smart_view_boards_all()}
              />
            </Field.Field>
            <Field.Field class="min-w-0">
              <Field.FieldLabel id="smart-view-completion-label"
                >{m.smart_view_completion()}</Field.FieldLabel
              >
              <MultiSelect
                id="smart-view-completion"
                label_id="smart-view-completion-label"
                options={completion_options}
                bind:selected={draft.filter.completion}
                empty_label={m.smart_view_any()}
              />
            </Field.Field>
            <Field.Field class="min-w-0">
              <Field.FieldLabel id="smart-view-priority-label"
                >{m.task_priority()}</Field.FieldLabel
              >
              <MultiSelect
                id="smart-view-priority"
                label_id="smart-view-priority-label"
                options={priority_options}
                bind:selected={draft.filter.priorities}
                empty_label={m.smart_view_any()}
              />
            </Field.Field>
            {@render single_select(
              "smart-view-due",
              m.smart_view_due(),
              due_options,
              draft.filter.due,
              (due) => (draft.filter.due = due),
            )}
            {@render single_select(
              "smart-view-waiting",
              m.smart_view_waiting(),
              waiting_options,
              draft.filter.waiting ?? "any",
              (waiting) =>
                (draft.filter.waiting = waiting === "any" ? null : waiting),
            )}
            {@render single_select(
              "smart-view-recurring",
              m.smart_view_recurring(),
              recurring_options,
              recurring_value,
              (value) =>
                (draft.filter.recurring =
                  value === "any" ? null : value === "only"),
            )}
          </div>
          <Field.Field>
            <Field.FieldLabel>{m.task_labels()}</Field.FieldLabel>
            <TagInput
              bind:tags={draft.filter.labels}
              maxTags={100}
              suggestions={label_suggestions}
              placeholder={m.smart_view_labels_placeholder()}
            />
          </Field.Field>
          <div class="grid gap-4 sm:grid-cols-2">
            {@render single_select(
              "smart-view-sort",
              m.smart_view_sort(),
              sort_options,
              draft.sort,
              (sort) => (draft.sort = sort),
            )}
            {@render single_select(
              "smart-view-group",
              m.smart_view_group(),
              group_options,
              draft.group,
              (group) => (draft.group = group),
            )}
          </div>
        </div>
      </ScrollArea>
      <Dialog.Footer class="border-t p-4">
        <Button
          type="button"
          variant="outline"
          disabled={saving}
          onclick={() => (open = false)}>{m.common_cancel()}</Button
        >
        <Button type="submit" disabled={saving}
          >{view ? m.common_save() : m.common_create()}</Button
        >
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
