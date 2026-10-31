<script lang="ts">
  import { m } from "#lib/paraglide/messages.js";
  import { formatNumber } from "#lib/i18n/index.js";
  import { translateCommandError } from "#lib/command-errors.js";
  import { DAILY_PLAN_ID, open_floating_focus } from "#lib/floating-focuses.js";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { untrack } from "svelte";
  import type { ComponentProps } from "svelte";
  import { DragDropProvider, PointerSensor } from "@dnd-kit-svelte/svelte";
  import { distance_activation_constraint } from "../dnd/activation";
  import DailyPlanItem from "./daily_plan_item.svelte";
  import { toast } from "svelte-sonner";
  import { Button } from "#lib/components/ui/button/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import XIcon from "@lucide/svelte/icons/x";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import SunIcon from "@lucide/svelte/icons/sun";
  import SearchIcon from "@lucide/svelte/icons/search";
  import LoaderCircleIcon from "@lucide/svelte/icons/loader-circle";
  import PictureInPicture2Icon from "@lucide/svelte/icons/picture-in-picture-2";
  import { Checkbox } from "#lib/components/ui/checkbox/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import DueBadge from "../task/due_badge.svelte";
  import PriorityIndicator from "../task/priority_indicator.svelte";
  import MultiSelect from "../focus/multi_select.svelte";
  import CheckIcon from "@lucide/svelte/icons/check";
  import type { AllTaskItem } from "../../board-base.svelte";
  import {
    deserialize_task_summary,
    get_task_id,
    priority_label,
    TASK_PRIORITIES,
    type TaskSummarySerialized,
  } from "../../type/task.svelte";
  import { create_focus, type FocusDue } from "../../type/focus";
  import { load_focus_results } from "../focus/focus-results";

  let {
    onOpenTask,
    boards,
    paused = false,
    floating = false,
    onCountChange,
  }: {
    onOpenTask: (item: AllTaskItem) => void;
    boards: { id: number; name: string; shared_role: string }[];
    paused?: boolean;
    floating?: boolean;
    onCountChange?: (count: number) => void;
  } = $props();
  const MAX_TASKS = 100;
  let items = $state<AllTaskItem[]>([]);
  let loaded = $state(false);
  let failed = $state(false);
  let busy = $state(false);
  let choosing = $state(false);
  let picker = $state(create_focus(""));
  let picker_viewport = $state<HTMLDivElement | null>(null);
  let search_at = new Date();
  const picker_key = $derived(JSON.stringify(picker.filter));
  const filtered = $derived(
    Boolean(
      picker.filter.query ||
      picker.filter.board_ids?.length ||
      picker.filter.priorities.length ||
      picker.filter.due !== "all",
    ),
  );
  const board_options = $derived(
    boards.map((board) => ({ value: board.id, label: board.name })),
  );
  const priority_options = $derived([
    ...[...TASK_PRIORITIES].reverse().map((priority) => ({
      value: priority,
      label: priority_label(priority),
    })),
    { value: "none" as const, label: m.task_priority_none() },
  ]);
  const due_options = $derived<{ value: FocusDue; label: string }[]>([
    { value: "all", label: m.focus_due_all() },
    { value: "overdue", label: m.focus_due_overdue() },
    { value: "today", label: m.focus_due_today() },
    { value: "by_today", label: m.focus_due_by_today() },
    { value: "next_7_days", label: m.focus_due_next_7_days() },
    { value: "none", label: m.focus_due_none() },
  ]);
  let candidates = $state<AllTaskItem[]>([]);
  let cursor = $state<string | null>(null);
  let searching = $state(false);
  let search_failed = $state(false);
  let plan_request = 0;
  let search_request = 0;
  const key = (item: AllTaskItem) => `${item.board_id}:${item.task.id}`;
  const selected = $derived(new Set(items.map(key)));
  const completed = $derived(items.filter((item) => item.done).length);
  $effect(() => {
    if (loaded) {
      onCountChange?.(items.length);
    }
  });
  const sensors = [
    PointerSensor.configure({
      activationConstraints: () => [distance_activation_constraint(8)] as never,
    }),
  ];
  let drag_items = $state.raw<AllTaskItem[] | null>(null);
  let drag_base: AllTaskItem[] | null = null;
  const shown_items = $derived(drag_items ?? items);
  type DragProps = ComponentProps<typeof DragDropProvider>;

  function drag_start() {
    drag_base = items;
    drag_items = items;
  }

  function drag_over(
    event: Parameters<NonNullable<DragProps["onDragOver"]>>[0],
  ) {
    const { source, target } = event.operation;
    if (
      source?.type !== "daily-plan-task" ||
      target?.type !== "daily-plan-task" ||
      !drag_items ||
      busy
    ) {
      return;
    }
    const from = drag_items.findIndex((item) => key(item) === source.id);
    const to = drag_items.findIndex((item) => key(item) === target.id);
    if (from < 0 || to < 0 || from === to) {
      return;
    }
    const next = [...drag_items];
    next.splice(to, 0, ...next.splice(from, 1));
    drag_items = next;
  }

  async function drag_end(
    event: Parameters<NonNullable<DragProps["onDragEnd"]>>[0],
  ) {
    const next = drag_items;
    try {
      if (
        !event.canceled &&
        event.operation.target?.type === "daily-plan-task" &&
        next &&
        drag_base === items &&
        next.some((item, index) => key(item) !== key(items[index]))
      ) {
        await save(next);
      }
    } finally {
      drag_items = null;
      drag_base = null;
    }
  }
  let now = $state(new Date());

  async function float_plan() {
    try {
      await open_floating_focus(DAILY_PLAN_ID);
    } catch (error) {
      toast.error(translateCommandError(error));
    }
  }

  async function refresh() {
    now = new Date();
    const request = ++plan_request;
    try {
      const rows = await invoke<
        Array<{
          board_id: number;
          board_name: string;
          column_name: string;
          done: boolean;
          task: TaskSummarySerialized;
        }>
      >("get_daily_plan");
      if (request !== plan_request) {
        return;
      }
      items = rows.map((row) => ({
        ...row,
        archived_at: undefined,
        task: deserialize_task_summary(row.task),
      }));
      loaded = true;
      failed = false;
    } catch (error) {
      if (request !== plan_request) {
        return;
      }
      failed = true;
      toast.error(translateCommandError(error));
    }
  }

  async function save(next: AllTaskItem[]) {
    if (busy || !loaded || next.length > MAX_TASKS) {
      return;
    }
    busy = true;
    ++plan_request;
    try {
      await invoke("set_daily_plan", {
        tasks: next.map((item) => ({
          board_id: item.board_id,
          task_id: get_task_id(item.task.id),
        })),
      });
      items = next;
    } catch (error) {
      toast.error(translateCommandError(error));
    } finally {
      busy = false;
    }
  }

  async function complete(item: AllTaskItem) {
    if (busy) {
      return;
    }
    busy = true;
    try {
      await invoke("complete_board_tasks", {
        boardId: item.board_id,
        taskIds: [get_task_id(item.task.id)],
      });
    } catch (error) {
      toast.error(translateCommandError(error));
    } finally {
      busy = false;
      await refresh();
    }
  }

  async function find_tasks(reset: boolean) {
    const request = ++search_request;
    searching = true;
    search_failed = false;
    if (reset) {
      candidates = [];
      cursor = null;
      search_at = new Date();
      picker_viewport?.scrollTo({ top: 0 });
    }
    const view = $state.snapshot(picker);
    try {
      const page = await load_focus_results(
        view,
        search_at,
        reset ? null : cursor,
        50,
      );
      if (request !== search_request) {
        return;
      }
      candidates = reset ? page.items : [...candidates, ...page.items];
      cursor = page.next_cursor;
    } catch (error) {
      if (request !== search_request) {
        return;
      }
      search_failed = true;
      toast.error(translateCommandError(error));
    } finally {
      if (request === search_request) {
        searching = false;
      }
    }
  }

  $effect(() => {
    if (!choosing) {
      return;
    }
    void picker_key;
    ++search_request;
    untrack(() => {
      candidates = [];
      cursor = null;
      searching = true;
    });
    const timer = setTimeout(() => untrack(() => void find_tasks(true)), 200);
    return () => {
      clearTimeout(timer);
      ++search_request;
    };
  });

  $effect(() => {
    if (!paused) {
      untrack(() => void refresh());
    }
  });

  $effect(() => {
    const stops = ["cardbe:data-changed", "cardbe:daily-plan-changed"].map(
      (event) =>
        listen(event, () => {
          if (!busy) {
            void refresh();
          }
        }),
    );
    return () => {
      ++plan_request;
      for (const stop of stops) {
        void stop.then((fn) => fn());
      }
    };
  });
</script>

<svelte:window
  onfocus={() => {
    if (!busy) void refresh();
  }}
/>

{#snippet choose_button()}
  {#if !floating}
    <Dialog.Trigger>
      {#snippet child({ props })}
        <Button {...props} size="sm" disabled={!loaded || failed}>
          <PlusIcon />{m.daily_plan_choose()}
        </Button>
      {/snippet}
    </Dialog.Trigger>
  {/if}
{/snippet}

{#snippet task_meta(item: AllTaskItem)}
  <span
    class="mt-1 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground"
  >
    <span class="truncate">{item.board_name} &middot; {item.column_name}</span>
    {#if item.task.priority && !item.done}<PriorityIndicator
        priority={item.task.priority}
        compact
      />{/if}
    {#if item.task.due_time && !item.done}<DueBadge
        due_time={item.task.due_time}
        at_midnight={item.task.due_at_midnight}
        {now}
      />{/if}
  </span>
{/snippet}

<Dialog.Root bind:open={choosing}>
  <section
    aria-labelledby="daily-plan-title"
    class={floating
      ? "overflow-hidden bg-card"
      : "overflow-hidden rounded-xl border bg-card shadow-xs"}
  >
    <header
      class={floating
        ? "sr-only"
        : "flex flex-wrap items-center gap-3 px-5 pt-5 sm:px-6"}
    >
      {#if !floating}
        <div
          class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary"
        >
          <SunIcon class="size-5" aria-hidden="true" />
        </div>
      {/if}
      <div class={floating ? "sr-only" : "min-w-0 flex-1"}>
        <h2 id="daily-plan-title" class="truncate font-semibold tracking-tight">
          {m.daily_plan_title()}
        </h2>
        <p class="mt-0.5 text-xs text-muted-foreground">
          {m.daily_plan_subtitle()}
        </p>
      </div>
      {#if busy && !choosing}
        <span role="status" class="text-muted-foreground">
          <LoaderCircleIcon class="size-4 animate-spin" aria-hidden="true" />
          <span class="sr-only">{m.daily_plan_saving()}</span>
        </span>
      {/if}
      {#if loaded && items.length > 0}{@render choose_button()}{/if}
      {#if !floating}
        <Button
          variant="ghost"
          size="icon-sm"
          class="shrink-0 text-muted-foreground"
          onclick={() => void float_plan()}
          aria-label={m.focus_float_on_desktop()}
          title={m.focus_float_on_desktop()}><PictureInPicture2Icon /></Button
        >
      {/if}
    </header>

    {#if failed}
      <div class="space-y-3 p-6">
        <p role="alert" class="text-sm text-destructive">
          {m.daily_plan_failed()}
        </p>
        <Button variant="outline" size="sm" onclick={() => void refresh()}
          >{m.daily_plan_retry()}</Button
        >
      </div>
    {:else if !loaded}
      <div
        role="status"
        class="flex items-center justify-center gap-2 py-12 text-sm text-muted-foreground"
      >
        <LoaderCircleIcon class="size-4 animate-spin" />{m.board_loading()}
      </div>
    {:else if items.length === 0}
      <div class="flex flex-col items-center px-6 py-10 text-center">
        <h3 class="text-sm font-medium">{m.daily_plan_empty_title()}</h3>
        <p class="mb-5 mt-2 text-sm text-muted-foreground">
          {floating ? m.daily_plan_float_empty() : m.daily_plan_empty()}
        </p>
        {@render choose_button()}
      </div>
    {:else}
      {#if !floating}
        <div class="mx-5 mb-2 mt-5 flex items-center gap-3 sm:mx-6">
          <div
            class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted"
            aria-hidden="true"
          >
            <div
              class="h-full rounded-full bg-primary transition-[width] duration-300 motion-reduce:transition-none"
              style:width={(completed / items.length) * 100 + "%"}
            ></div>
          </div>
          <span class="shrink-0 text-xs tabular-nums text-muted-foreground"
            >{m.daily_plan_progress({
              done: formatNumber(completed),
              total: formatNumber(items.length),
            })}</span
          >
        </div>
      {/if}
      <DragDropProvider
        {sensors}
        onDragStart={drag_start}
        onDragOver={drag_over}
        onDragEnd={drag_end}
      >
        <ol
          class={floating
            ? "divide-y divide-border/60"
            : "divide-y divide-border/60 px-3 pb-2 sm:px-4"}
        >
          {#each shown_items as item, index (key(item))}
            <DailyPlanItem
              id={key(item)}
              {index}
              compact={floating}
              disabled={busy || failed || choosing}
            >
              <Checkbox
                checked={Boolean(item.done)}
                disabled={busy ||
                  item.done ||
                  boards.find((board) => board.id === item.board_id)
                    ?.shared_role === "viewer"}
                aria-label={item.done ? m.common_done() : m.task_complete()}
                onCheckedChange={() => void complete(item)}
                class="size-5 rounded-full"
              />
              <button
                type="button"
                class="min-w-0 flex-1 rounded-md py-1 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                onclick={() => onOpenTask(item)}
              >
                <span
                  class={[
                    "block truncate text-sm font-medium",
                    item.done && "text-muted-foreground line-through",
                  ]}>{item.task.title || m.task_untitled()}</span
                >
                {@render task_meta(item)}
              </button>
            </DailyPlanItem>
          {/each}
        </ol>
      </DragDropProvider>
      {#if completed > 0}
        <footer class="flex justify-end border-t bg-muted/20 px-4 py-2">
          <Button
            variant="ghost"
            size="sm"
            disabled={busy}
            onclick={() => void save(items.filter((item) => !item.done))}
            >{m.daily_plan_clear_done()}</Button
          >
        </footer>
      {/if}
    {/if}
  </section>

  <Dialog.Content
    class="grid h-[min(44rem,85dvh)] grid-rows-[auto_minmax(0,1fr)_auto] gap-0 overflow-hidden p-0 sm:max-w-xl"
  >
    <div class="min-w-0">
      <Dialog.Header class="shrink-0 px-6 pb-4 pt-6 text-left">
        <Dialog.Title>{m.daily_plan_choose()}</Dialog.Title>
        <Dialog.Description>{m.daily_plan_picker_help()}</Dialog.Description>
      </Dialog.Header>
      <div class="relative mx-6 mb-3 shrink-0">
        <SearchIcon
          class="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
          aria-hidden="true"
        /><Input
          bind:value={picker.filter.query}
          class="ps-9"
          aria-label={m.daily_plan_search()}
          placeholder={m.daily_plan_search()}
        />
      </div>
      <div class="mx-6 mb-3 grid grid-cols-3 gap-2">
        <div class="min-w-0 space-y-1.5">
          <span
            id="daily-plan-board-label"
            class="text-xs font-medium text-muted-foreground"
            >{m.focus_boards()}</span
          >
          <MultiSelect
            id="daily-plan-boards"
            label_id="daily-plan-board-label"
            options={board_options}
            bind:selected={
              () => picker.filter.board_ids ?? [],
              (value) => {
                picker.filter.board_ids = value.length ? value : null;
              }
            }
            empty_label={m.focus_boards_all()}
          />
        </div>
        <div class="min-w-0 space-y-1.5">
          <span
            id="daily-plan-priority-label"
            class="text-xs font-medium text-muted-foreground"
            >{m.task_priority()}</span
          >
          <MultiSelect
            id="daily-plan-priorities"
            label_id="daily-plan-priority-label"
            options={priority_options}
            bind:selected={picker.filter.priorities}
            empty_label={m.focus_any()}
          />
        </div>
        <div class="min-w-0 space-y-1.5">
          <span
            id="daily-plan-due-label"
            class="text-xs font-medium text-muted-foreground"
            >{m.focus_due()}</span
          >
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  variant="outline"
                  class="h-9 w-full min-w-0 justify-between font-normal"
                  aria-labelledby="daily-plan-due-label daily-plan-due-value"
                  ><span id="daily-plan-due-value" class="truncate"
                    >{due_options.find(
                      (option) => option.value === picker.filter.due,
                    )?.label}</span
                  ><ChevronDownIcon
                    class="size-4 shrink-0"
                    aria-hidden="true"
                  /></Button
                >
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="start">
              <DropdownMenu.RadioGroup bind:value={picker.filter.due}>
                {#each due_options as option (option.value)}<DropdownMenu.RadioItem
                    value={option.value}>{option.label}</DropdownMenu.RadioItem
                  >{/each}
              </DropdownMenu.RadioGroup>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
      </div>
      {#if filtered}<div class="mx-6 mb-3 flex justify-end">
          <Button
            variant="ghost"
            size="sm"
            onclick={() => (picker = create_focus(""))}
            ><XIcon />{m.daily_plan_clear_filters()}</Button
          >
        </div>{/if}
      {#if items.length >= MAX_TASKS}<p
          class="px-6 pb-3 text-sm text-muted-foreground"
        >
          {m.daily_plan_limit({ count: formatNumber(MAX_TASKS) })}
        </p>{/if}
    </div>
    <ScrollArea
      bind:viewport={picker_viewport}
      orientation="vertical"
      class="h-full min-h-0 overflow-hidden border-y"
    >
      <ul class="space-y-1 p-3">
        {#each candidates as item (key(item))}
          {@const added = selected.has(key(item))}
          <li>
            <Button
              variant="ghost"
              class={[
                "h-auto w-full justify-start gap-3 px-3 py-3 text-left whitespace-normal",
                added && "bg-primary/5",
              ]}
              aria-pressed={added}
              aria-disabled={busy || undefined}
              disabled={failed ||
                searching ||
                (!added && items.length >= MAX_TASKS)}
              onclick={() =>
                void save(
                  added
                    ? items.filter((candidate) => key(candidate) !== key(item))
                    : [...items, item],
                )}
            >
              <span
                class={[
                  "flex size-5 shrink-0 items-center justify-center rounded-full border",
                  added
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-input text-muted-foreground",
                ]}
              >
                {#if added}<CheckIcon class="size-3.5" />{:else}<PlusIcon
                    class="size-3.5"
                  />{/if}
              </span>
              <span class="min-w-0 flex-1"
                ><span class="block truncate text-sm font-medium"
                  >{item.task.title || m.task_untitled()}</span
                >{@render task_meta(item)}</span
              >
              {#if added}<span class="shrink-0 text-xs text-primary"
                  >{m.daily_plan_added()}</span
                >{/if}
            </Button>
          </li>
        {/each}
      </ul>
      {#if searching}<p
          role="status"
          class="flex items-center justify-center gap-2 p-6 text-sm text-muted-foreground"
        >
          <LoaderCircleIcon class="size-4 animate-spin" />{m.board_loading()}
        </p>
      {:else if search_failed}<div class="p-6 text-center">
          <Button variant="outline" onclick={() => void find_tasks(true)}
            >{m.daily_plan_retry()}</Button
          >
        </div>
      {:else if candidates.length === 0}<p
          class="p-8 text-center text-sm text-muted-foreground"
        >
          {m.focus_empty()}
        </p>{/if}
      {#if cursor}<div class="p-3 text-center">
          <Button
            variant="ghost"
            disabled={searching}
            onclick={() => void find_tasks(false)}>{m.focus_load_more()}</Button
          >
        </div>{/if}
    </ScrollArea>
    <Dialog.Footer
      class="flex-row items-center justify-between gap-3 px-6 py-4 sm:justify-between"
    >
      <p
        role="status"
        class="flex items-center gap-2 text-xs tabular-nums text-muted-foreground"
      >
        {m.daily_plan_selected({ count: formatNumber(items.length) })}
        <span class="flex size-3 items-center justify-center">
          {#if busy}<LoaderCircleIcon
              class="size-3 animate-spin"
              aria-hidden="true"
            /><span class="sr-only">{m.daily_plan_saving()}</span>{/if}
        </span>
      </p>
      <Dialog.Close
        >{#snippet child({ props })}<Button
            {...props}
            size="sm"
            disabled={busy}
            class="disabled:opacity-100">{m.daily_plan_finish()}</Button
          >{/snippet}</Dialog.Close
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
