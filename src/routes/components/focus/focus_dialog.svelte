<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import "#lib/i18n/locale.svelte.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as Field from "#lib/components/ui/field/index.js";
  import * as Select from "#lib/components/ui/select/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { TagInput } from "#lib/components/ui/tag-input/index.js";

  import {
    clone_focus,
    create_focus,
    MAX_FOCUS_NAME_LENGTH,
    type Focus,
    type FocusDue,
    type FocusGroup,
    type FocusSort,
  } from "../../type/focus";
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
    template = null,
    boards,
    label_suggestions,
    onSave,
  }: {
    open: boolean;
    /** The view to edit, or null to create one. */
    view: Focus | null;
    /** Conditions a new view starts from, when `view` is null. */
    template?: Focus | null;
    boards: { id: number; name: string }[];
    label_suggestions: string[];
    onSave: (view: Focus) => Promise<boolean>;
  } = $props();

  let draft = $state<Focus>(create_focus(""));
  let invalid_name = $state(false);
  let saving = $state(false);

  $effect(() => {
    if (open) {
      draft = view
        ? clone_focus(view)
        : template
          ? { ...clone_focus(template), id: create_focus("").id }
          : create_focus("");
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
    { value: "open", label: m.focus_completion_open() },
    { value: "done", label: m.focus_completion_done() },
    { value: "archived", label: m.focus_completion_archived() },
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
  const due_options = $derived<{ value: FocusDue; label: string }[]>([
    { value: "all", label: m.focus_due_all() },
    { value: "overdue", label: m.focus_due_overdue() },
    { value: "today", label: m.focus_due_today() },
    { value: "by_today", label: m.focus_due_by_today() },
    { value: "next_7_days", label: m.focus_due_next_7_days() },
    { value: "none", label: m.focus_due_none() },
  ]);
  const waiting_options = $derived<
    { value: WaitingFilter | "any"; label: string }[]
  >([
    { value: "any", label: m.focus_any() },
    { value: "waiting", label: m.focus_waiting_waiting() },
    { value: "not_waiting", label: m.focus_waiting_not_waiting() },
    { value: "waited_on", label: m.focus_waiting_waited_on() },
  ]);
  const recurring_options = $derived([
    { value: "any", label: m.focus_any() },
    { value: "only", label: m.focus_recurring_only() },
    { value: "exclude", label: m.focus_recurring_exclude() },
  ]);
  const sort_options = $derived<{ value: FocusSort; label: string }[]>([
    { value: "priority", label: m.focus_sort_priority() },
    { value: "due", label: m.focus_sort_due() },
    { value: "created", label: m.focus_sort_created() },
    { value: "title", label: m.focus_sort_title() },
  ]);
  const group_options = $derived<{ value: FocusGroup; label: string }[]>([
    { value: "none", label: m.focus_group_none() },
    { value: "board", label: m.focus_group_board() },
    { value: "priority", label: m.focus_group_priority() },
    { value: "due", label: m.focus_group_due() },
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
      const saved: Focus = {
        ...clone_focus(draft),
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
      <Dialog.Title>{view ? m.focus_edit_title() : m.focus_new()}</Dialog.Title>
      <Dialog.Description>{m.focus_dialog_description()}</Dialog.Description>
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
            <Field.FieldLabel for="focus-name"
              >{m.focus_name()}</Field.FieldLabel
            >
            <Input
              id="focus-name"
              bind:value={draft.name}
              maxlength={MAX_FOCUS_NAME_LENGTH}
              aria-invalid={invalid_name}
              oninput={() => (invalid_name = false)}
              autofocus
            />
            {#if invalid_name}
              <Field.Error>{m.focus_name_required()}</Field.Error>
            {/if}
          </Field.Field>
          <Field.Field>
            <Field.FieldLabel for="focus-query"
              >{m.focus_query()}</Field.FieldLabel
            >
            <Input
              id="focus-query"
              bind:value={draft.filter.query}
              placeholder={m.focus_query_placeholder()}
              maxlength={500}
            />
          </Field.Field>
          <div class="grid gap-4 sm:grid-cols-2">
            <Field.Field class="min-w-0">
              <Field.FieldLabel id="focus-boards-label"
                >{m.focus_boards()}</Field.FieldLabel
              >
              <MultiSelect
                id="focus-boards"
                label_id="focus-boards-label"
                options={boards.map((board) => ({
                  value: board.id,
                  label: board.name,
                }))}
                bind:selected={
                  () => board_ids,
                  (ids) =>
                    (draft.filter.board_ids = ids.length > 0 ? ids : null)
                }
                empty_label={m.focus_boards_all()}
              />
            </Field.Field>
            <Field.Field class="min-w-0">
              <Field.FieldLabel id="focus-completion-label"
                >{m.focus_completion()}</Field.FieldLabel
              >
              <MultiSelect
                id="focus-completion"
                label_id="focus-completion-label"
                options={completion_options}
                bind:selected={draft.filter.completion}
                empty_label={m.focus_any()}
              />
            </Field.Field>
            <Field.Field class="min-w-0">
              <Field.FieldLabel id="focus-priority-label"
                >{m.task_priority()}</Field.FieldLabel
              >
              <MultiSelect
                id="focus-priority"
                label_id="focus-priority-label"
                options={priority_options}
                bind:selected={draft.filter.priorities}
                empty_label={m.focus_any()}
              />
            </Field.Field>
            {@render single_select(
              "focus-due",
              m.focus_due(),
              due_options,
              draft.filter.due,
              (due) => (draft.filter.due = due),
            )}
            {@render single_select(
              "focus-waiting",
              m.focus_waiting(),
              waiting_options,
              draft.filter.waiting ?? "any",
              (waiting) =>
                (draft.filter.waiting = waiting === "any" ? null : waiting),
            )}
            {@render single_select(
              "focus-recurring",
              m.focus_recurring(),
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
              placeholder={m.focus_labels_placeholder()}
            />
          </Field.Field>
          <div class="grid gap-4 sm:grid-cols-2">
            {@render single_select(
              "focus-sort",
              m.focus_sort(),
              sort_options,
              draft.sort,
              (sort) => (draft.sort = sort),
            )}
            {@render single_select(
              "focus-group",
              m.focus_group(),
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
