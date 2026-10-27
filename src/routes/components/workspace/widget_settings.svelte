<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { logger } from "#lib/logger.js";
  import { toast } from "svelte-sonner";
  import * as m from "#lib/paraglide/messages.js";
  import {
    parseCommandError,
    translateCommandError,
    type CommandError,
  } from "#lib/command-errors.js";
  import {
    desktop_widgets,
    hide_floating_widget,
    open_widget,
    read_floating_visibility,
    read_widgets,
    remove_widget,
    type DesktopWidgetRef,
    type WidgetPlacement,
  } from "#lib/desktop-widget.svelte.js";
  import * as Card from "#lib/components/ui/card/index.js";
  import * as Dialog from "#lib/components/ui/dialog/index.js";
  import * as Tabs from "#lib/components/ui/tabs/index.js";
  import * as DropdownMenu from "#lib/components/ui/dropdown-menu/index.js";
  import { ScrollArea } from "#lib/components/ui/scroll-area/index.js";
  import { Badge } from "#lib/components/ui/badge/index.js";
  import { Button } from "#lib/components/ui/button/index.js";
  import { Input } from "#lib/components/ui/input/index.js";
  import { Label } from "#lib/components/ui/label/index.js";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import CheckIcon from "@lucide/svelte/icons/check";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import PanelsTopLeftIcon from "@lucide/svelte/icons/panels-top-left";
  import PictureInPicture2Icon from "@lucide/svelte/icons/picture-in-picture-2";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import { board } from "../../board.svelte";
  import type { Focus } from "../../type/focus";
  import type { ColumnSerialized } from "../../type/column.svelte";

  // One list of desktop widgets and floating windows. Adding happens in a
  // separate dialog that picks a source, then a placement.
  let {
    active,
    focuses,
    onPrepareFocus,
  }: {
    /** Whether the settings section is visible, so the lists stay current. */
    active: boolean;
    focuses: Focus[];
    onPrepareFocus: () => Promise<void>;
  } = $props();
  type Entry = { ref: DesktopWidgetRef; placement: WidgetPlacement };
  let entries = $state<Entry[]>([]);
  /** Keys of floating windows that are currently hidden. */
  let hidden = $state<string[]>([]);
  let adding = $state(false);
  let tab = $state<DesktopWidgetRef["kind"]>("focus");
  let selected_board = $state("");
  let query = $state("");
  let requested_task = $state<number | null>(null);
  let columns = $state<Record<number, ColumnSerialized[]>>({});
  let loading = $state(false);
  let working = $state(false);
  let load_error = $state<CommandError | null>(null);
  let action_error = $state<CommandError | null>(null);
  let retry = $state(0);

  const placements = $derived<WidgetPlacement[]>(
    desktop_widgets.supported ? ["desktop", "floating"] : ["floating"],
  );

  function key(ref: DesktopWidgetRef) {
    return ref.kind === "focus"
      ? `focus:${ref.id}`
      : `task:${ref.boardId}:${ref.taskId}`;
  }
  function read_entries(): Entry[] {
    return placements.flatMap((placement) =>
      read_widgets(placement).map((ref) => ({ ref, placement })),
    );
  }
  // Newly opened windows show themselves once loaded, so callers that just
  // opened one update `hidden` directly instead of asking here.
  async function refresh_visibility() {
    const refs = entries
      .filter((entry) => entry.placement === "floating")
      .map((entry) => entry.ref);
    try {
      const visible = await read_floating_visibility(refs);
      hidden = refs.filter((_, index) => !visible[index]).map(key);
    } catch (error) {
      logger.warn("widget_settings.visibility.failed", error);
    }
  }
  function refresh() {
    entries = read_entries();
    void refresh_visibility();
  }
  function added(ref: DesktopWidgetRef, placement: WidgetPlacement) {
    return entries.some(
      (entry) => entry.placement === placement && key(entry.ref) === key(ref),
    );
  }
  function title(ref: DesktopWidgetRef) {
    if (ref.kind === "focus") {
      return (
        focuses.find((view) => view.id === ref.id)?.name ??
        m.desktop_widget_source_unavailable()
      );
    }
    const task = columns[ref.boardId]
      ?.flatMap((column) => column.tasks)
      .find((task) => task.id === ref.taskId);
    return task
      ? task.title || m.task_untitled()
      : m.desktop_widget_source_unavailable();
  }
  function subtitle(ref: DesktopWidgetRef) {
    return ref.kind === "focus"
      ? m.desktop_widget_focus_type()
      : (board.boards.find((item) => item.id === ref.boardId)?.name ??
          m.desktop_widget_source_unavailable());
  }

  $effect(() => {
    if (!active) {
      return;
    }
    untrack(() => {
      refresh();
      const requested = desktop_widgets.requested;
      desktop_widgets.requested = null;
      if (requested) {
        tab = requested.kind;
        adding = true;
      }
      requested_task = requested?.kind === "task" ? requested.taskId : null;
      if (requested?.kind === "task") {
        selected_board = String(requested.boardId);
      } else if (!selected_board) {
        selected_board = String(
          board.active_board_id ?? board.boards[0]?.id ?? "",
        );
      }
      query = "";
      action_error = null;
    });
  });

  $effect(() => {
    if (!active) {
      return;
    }
    void retry;
    const ids = [
      ...new Set([
        ...entries
          .map((entry) => entry.ref)
          .filter((ref) => ref.kind === "task")
          .map((ref) => ref.boardId),
        ...(adding && tab === "task" && selected_board
          ? [Number(selected_board)]
          : []),
      ]),
    ];
    let cancelled = false;
    loading = true;
    load_error = null;
    void Promise.allSettled(
      ids.map((boardId) =>
        invoke<ColumnSerialized[]>("get_board_columns", { boardId }),
      ),
    ).then((results) => {
      if (cancelled) {
        return;
      }
      const loaded: Record<number, ColumnSerialized[]> = {};
      for (const [index, result] of results.entries()) {
        if (result.status === "fulfilled") {
          loaded[ids[index]] = result.value;
        } else {
          load_error = parseCommandError(result.reason) ?? {
            code: "INTERNAL_ERROR",
          };
        }
      }
      columns = loaded;
      loading = false;
    });
    return () => {
      cancelled = true;
    };
  });

  const tasks = $derived(
    (columns[Number(selected_board)] ?? [])
      .flatMap((column) =>
        column.tasks.map((task) => ({ task, column: column.name })),
      )
      .filter(({ task }) =>
        task.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      )
      .sort(
        (left, right) =>
          Number(right.task.id === requested_task) -
          Number(left.task.id === requested_task),
      ),
  );

  async function add(ref: DesktopWidgetRef, placement: WidgetPlacement) {
    if (working) {
      return;
    }
    working = true;
    action_error = null;
    try {
      if (ref.kind === "focus") {
        await onPrepareFocus();
      }
      await open_widget(ref, placement);
      entries = read_entries();
      hidden = hidden.filter((item) => item !== key(ref));
      // Floating windows appear on top; desktop widgets are behind Cardbe.
      if (placement === "desktop") {
        toast.success(m.desktop_widget_added());
      }
    } catch (error) {
      action_error = parseCommandError(error) ?? { code: "INTERNAL_ERROR" };
    } finally {
      working = false;
    }
  }
  async function hide(entry: Entry) {
    if (working) {
      return;
    }
    working = true;
    action_error = null;
    try {
      await hide_floating_widget(entry.ref);
      hidden = [...hidden, key(entry.ref)];
    } catch (error) {
      action_error = parseCommandError(error) ?? { code: "INTERNAL_ERROR" };
    } finally {
      working = false;
    }
  }
  async function remove(entry: Entry) {
    if (working) {
      return;
    }
    working = true;
    action_error = null;
    try {
      await remove_widget(entry.ref, entry.placement);
      entries = read_entries();
    } catch (error) {
      action_error = parseCommandError(error) ?? { code: "INTERNAL_ERROR" };
    } finally {
      working = false;
    }
  }

  onMount(() => {
    // Widgets closed from their own window update the stored lists.
    const changed = (event: StorageEvent) => {
      if (
        active &&
        (event.key === null ||
          event.key === "cardbe.desktop-widgets" ||
          event.key === "cardbe.floating-tasks" ||
          event.key === "cardbe.floating-focuses")
      ) {
        refresh();
      }
    };
    // The tray toggle hides or shows every floating window.
    const visibility_changed = () => {
      if (active) {
        void refresh_visibility();
      }
    };
    const unlisten = listen(
      "cardbe:floating-visibility-changed",
      visibility_changed,
    );
    window.addEventListener("storage", changed);
    window.addEventListener("focus", visibility_changed);
    return () => {
      window.removeEventListener("storage", changed);
      window.removeEventListener("focus", visibility_changed);
      void unlisten.then((stop) => stop());
    };
  });
</script>

{#snippet add_buttons(
  ref: DesktopWidgetRef,
  name: string,
  unavailable: boolean,
)}
  <div class="flex shrink-0 gap-2">
    {#each placements as placement (placement)}
      {@const done = added(ref, placement)}
      <Button
        variant="outline"
        size="sm"
        disabled={working || unavailable || done}
        aria-label={placement === "desktop"
          ? m.widget_add_desktop_named({ name })
          : m.widget_add_floating_named({ name })}
        title={done ? m.desktop_widget_already_added() : undefined}
        onclick={() => void add(ref, placement)}
      >
        {#if done}<CheckIcon
          />{:else if placement === "desktop"}<PanelsTopLeftIcon
          />{:else}<PictureInPicture2Icon />{/if}
        {placement === "desktop"
          ? m.widget_placement_desktop()
          : m.widget_placement_floating()}
      </Button>
    {/each}
  </div>
{/snippet}

<div class="flex h-full min-h-0 flex-col gap-4 p-1">
  <div class="flex items-start gap-4">
    <div class="min-w-0 flex-1 space-y-1">
      <h3 class="text-sm font-medium">{m.desktop_widget_manager_title()}</h3>
      <p class="text-sm text-muted-foreground">
        {m.desktop_widget_manager_description()}
      </p>
    </div>
    <Button size="sm" class="shrink-0" onclick={() => (adding = true)}
      ><PlusIcon />{m.desktop_widget_add_new()}</Button
    >
  </div>
  {#if (action_error && !adding) || load_error}
    <div role="alert" class="flex items-center gap-2 text-sm text-destructive">
      <p class="min-w-0 flex-1">
        {translateCommandError((adding ? null : action_error) ?? load_error)}
      </p>
      {#if load_error}<Button
          variant="outline"
          size="sm"
          onclick={() => (retry += 1)}>{m.common_retry()}</Button
        >{/if}
    </div>
  {/if}
  <ScrollArea orientation="vertical" class="min-h-0 flex-1">
    <div class="space-y-2 p-1">
      {#each entries as entry (`${entry.placement}:${key(entry.ref)}`)}
        {@const name = title(entry.ref)}
        <Card.Root
          role="group"
          aria-label={name}
          class="flex-row items-center gap-3 p-3"
        >
          <div class="min-w-0 flex-1">
            <p class="truncate text-sm font-medium">{name}</p>
            <p class="truncate text-xs text-muted-foreground">
              {subtitle(entry.ref)}
            </p>
          </div>
          <Badge variant="secondary" class="shrink-0">
            {entry.placement === "desktop"
              ? m.widget_placement_desktop()
              : m.widget_placement_floating()}
          </Badge>
          {#if entry.placement === "floating"}
            {@const is_hidden = hidden.includes(key(entry.ref))}
            <Button
              variant="outline"
              size="sm"
              disabled={working}
              onclick={() =>
                void (is_hidden
                  ? add(entry.ref, entry.placement)
                  : hide(entry))}
              >{is_hidden
                ? m.desktop_widget_show()
                : m.desktop_widget_hide()}</Button
            >
          {/if}
          <Button
            variant="ghost"
            size="icon-sm"
            class="text-destructive hover:bg-destructive/10 hover:text-destructive"
            disabled={working}
            aria-label={entry.placement === "desktop"
              ? m.desktop_widget_remove_named({ name })
              : m.widget_remove_floating_named({ name })}
            title={entry.placement === "desktop"
              ? m.desktop_widget_remove()
              : m.widget_remove_floating_named({ name })}
            onclick={() => void remove(entry)}><Trash2Icon /></Button
          >
        </Card.Root>
      {:else}<p class="px-4 py-12 text-center text-sm text-muted-foreground">
          {m.desktop_widget_none()}
        </p>{/each}
    </div>
  </ScrollArea>
</div>

<Dialog.Root bind:open={adding}>
  <Dialog.Content class="flex h-[min(36rem,85vh)] flex-col sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{m.desktop_widget_add_new()}</Dialog.Title>
      <Dialog.Description
        >{m.desktop_widget_add_description()}</Dialog.Description
      >
    </Dialog.Header>
    {#if action_error}
      <p role="alert" class="text-sm text-destructive">
        {translateCommandError(action_error)}
      </p>
    {/if}
    <Tabs.Root bind:value={tab} class="flex min-h-0 flex-1 flex-col">
      <Tabs.List class="grid w-full grid-cols-2">
        <Tabs.Trigger value="focus"
          >{m.desktop_widget_focus_type()}</Tabs.Trigger
        >
        <Tabs.Trigger value="task">{m.desktop_widget_task_type()}</Tabs.Trigger>
      </Tabs.List>
      <Tabs.Content value="focus" class="min-h-0">
        <ScrollArea orientation="vertical" class="h-full">
          <div class="space-y-2 p-1">
            {#each focuses as view (view.id)}
              <Card.Root
                role="group"
                aria-label={view.name}
                class="flex-row items-center gap-3 p-3"
              >
                <p class="min-w-0 flex-1 truncate text-sm font-medium">
                  {view.name}
                </p>
                {@render add_buttons(
                  { kind: "focus", id: view.id },
                  view.name,
                  false,
                )}
              </Card.Root>
            {:else}<p
                class="px-4 py-12 text-center text-sm text-muted-foreground"
              >
                {m.desktop_widget_no_focus()}
              </p>{/each}
          </div>
        </ScrollArea>
      </Tabs.Content>
      <Tabs.Content value="task" class="flex min-h-0 flex-col gap-3">
        <div class="flex items-center gap-3">
          <Label for="widget-board">{m.board_title()}</Label>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button
                  {...props}
                  id="widget-board"
                  variant="outline"
                  class="min-w-0 flex-1 justify-between font-normal"
                >
                  <span class="truncate"
                    >{board.boards.find(
                      (item) => String(item.id) === selected_board,
                    )?.name ?? m.desktop_widget_source_unavailable()}</span
                  ><ChevronDownIcon />
                </Button>
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="start" class="max-h-64 min-w-56">
              <DropdownMenu.RadioGroup bind:value={selected_board}>
                {#each board.boards as item (item.id)}
                  <DropdownMenu.RadioItem value={String(item.id)}
                    >{item.name}</DropdownMenu.RadioItem
                  >
                {/each}
              </DropdownMenu.RadioGroup>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
        <Input
          bind:value={query}
          aria-label={m.desktop_widget_search_tasks()}
          placeholder={m.desktop_widget_search_tasks()}
        />
        <ScrollArea
          orientation="vertical"
          class="min-h-0 flex-1"
          aria-busy={loading}
        >
          <div class="space-y-2 p-1">
            {#each tasks as item (item.task.id)}
              {@const name = item.task.title || m.task_untitled()}
              <Card.Root
                role="group"
                aria-label={name}
                class="flex-row items-center gap-3 p-3"
              >
                <div class="min-w-0 flex-1">
                  <p class="truncate text-sm font-medium">{name}</p>
                  <p class="truncate text-xs text-muted-foreground">
                    {item.column}
                  </p>
                </div>
                {@render add_buttons(
                  {
                    kind: "task",
                    boardId: Number(selected_board),
                    taskId: item.task.id,
                  },
                  name,
                  loading,
                )}
              </Card.Root>
            {:else}<p
                class="px-4 py-12 text-center text-sm text-muted-foreground"
              >
                {loading ? m.app_loading() : m.desktop_widget_no_tasks()}
              </p>{/each}
          </div>
        </ScrollArea>
      </Tabs.Content>
    </Tabs.Root>
  </Dialog.Content>
</Dialog.Root>
