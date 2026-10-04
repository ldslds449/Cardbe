<script lang="ts">
  import {
    parseCommandError,
    translateCommandError,
  } from "$lib/command-errors";
  import { formatNumber } from "$lib/i18n";
  import { getLocale } from "$lib/i18n";
  import * as m from "$lib/paraglide/messages.js";
  import { logger } from "$lib/logger";
  import { invoke } from "@tauri-apps/api/core";
  import { toast } from "svelte-sonner";
  import PlusIcon from "@lucide/svelte/icons/plus";
  import PencilIcon from "@lucide/svelte/icons/pencil";
  import CalendarDaysIcon from "@lucide/svelte/icons/calendar-days";
  import ChevronDownIcon from "@lucide/svelte/icons/chevron-down";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import SearchIcon from "@lucide/svelte/icons/search";
  import Trash2Icon from "@lucide/svelte/icons/trash-2";
  import PowerIcon from "@lucide/svelte/icons/power";
  import CopyIcon from "@lucide/svelte/icons/copy";
  import CircleCheckIcon from "@lucide/svelte/icons/circle-check";
  import LayoutDashboardIcon from "@lucide/svelte/icons/layout-dashboard";
  import {
    CalendarDate,
    getLocalTimeZone,
    today,
  } from "@internationalized/date";
  import { Button } from "$lib/components/ui/button/index.js";
  import Calendar from "$lib/components/ui/calendar/calendar.svelte";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as AlertDialog from "$lib/components/ui/alert-dialog/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { TagInput } from "$lib/components/ui/tag-input/index.js";
  import * as Popover from "$lib/components/ui/popover/index.js";
  import {
    deserialize_column,
    type Column,
    type ColumnSerialized,
  } from "../../type/column.svelte";
  import type { BoardRole } from "../../board.svelte";
  import {
    build_share_snapshot,
    forget_managed_share,
    is_managed_share_enabled,
    load_managed_shares,
    publish_share,
    rebind_managed_share,
    revoke_share,
    resolve_share_selection,
    save_managed_share,
    share_content_signature,
    type ManagedShare,
  } from "../../share";

  let {
    open = $bindable(),
    columns,
    default_title,
    active_board_id,
    boards,
    onRetireShare,
    onShareRevokeError,
  }: {
    open: boolean;
    columns: Column[];
    default_title: string;
    active_board_id: number | null;
    boards: Array<{ id: number; name: string; shared_role?: BoardRole }>;
    onRetireShare?: (share_id: string) => void;
    onShareRevokeError?: (share_id: string, error: unknown) => void;
  } = $props();

  let share = $state<ManagedShare | null>(null);
  let title = $state("");
  let selected_column_ids = $state<string[]>([]);
  let selected_task_ids = $state<string[]>([]);
  let selected_labels = $state<string[]>([]);
  let task_search = $state("");
  let reuse_link_open = $state(false);
  let requested_link = $state("");
  let expiration_enabled = $state(true);
  let expiration_date = $state(today(getLocalTimeZone()).add({ days: 30 }));
  let expiration_open = $state(false);
  let context_menu_share_id = $state<string | null>(null);
  let publishing = $state(false);
  let revoking = $state(false);
  let delete_confirm_open = $state(false);
  let editing = $state(false);
  // A share can belong to any board, even while another board is open in the
  // workspace. Keep its content loaded locally so managing a link never
  // changes the user's current board.
  let selected_board_id = $state<number | null>(null);
  let loaded_board_id = $state<number | null>(null);
  const owned_boards = $derived(
    boards.filter(
      (board) => !board.shared_role || board.shared_role === "owner",
    ),
  );
  let loaded_columns = $state<Column[]>([]);
  let loading_board_content = $state(false);
  let board_content_generation = 0;
  const share_columns = $derived(
    loaded_board_id === active_board_id ? columns : loaded_columns,
  );
  let initialized_for_open = false;
  const all_managed_shares = $derived(
    load_managed_shares().filter((candidate) =>
      Number.isInteger(candidate.board_id),
    ),
  );
  const managed_shares = $derived(
    all_managed_shares.filter(
      (candidate) => candidate.board_id === active_board_id,
    ),
  );
  const legacy_share_count = $derived(
    load_managed_shares().filter(
      (candidate) => !Number.isInteger(candidate.board_id),
    ).length,
  );
  const legacy_shares = $derived(
    load_managed_shares().filter(
      (candidate) => !Number.isInteger(candidate.board_id),
    ),
  );
  function board_name(board_id: number | undefined): string {
    return (
      boards.find((board) => board.id === board_id)?.name ??
      m.ui_unknown_board()
    );
  }
  const share_date_formatter = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      year: "numeric",
      month: "short",
      day: "numeric",
    }),
  );
  const share_datetime_formatter = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    }),
  );

  async function load_board_content(board_id: number | null): Promise<boolean> {
    if (board_id === null) {
      return false;
    }
    const generation = ++board_content_generation;
    selected_board_id = board_id;
    if (board_id === active_board_id) {
      loaded_board_id = board_id;
      loaded_columns = columns;
      loading_board_content = false;
      return true;
    }
    loading_board_content = true;
    try {
      const serialized = await invoke<ColumnSerialized[]>("get_board_columns", {
        boardId: board_id,
      });
      if (generation !== board_content_generation) {
        return false;
      }
      loaded_columns = serialized.map(deserialize_column);
      loaded_board_id = board_id;
      return true;
    } catch (error) {
      logger.error("share.content_load.failed", error);
      if (generation !== board_content_generation) {
        return false;
      }
      console.error(m.ui_couldn_t_load_shared_board_content(), error);
      toast.error(translateCommandError(error));
      return false;
    } finally {
      if (generation === board_content_generation) {
        loading_board_content = false;
      }
    }
  }

  function load_share(
    candidate: ManagedShare | null,
    editing_state = candidate === null,
  ) {
    share = candidate;
    editing = editing_state;
    title = candidate?.title || `${default_title || "Cardbe"} board`;
    const available_ids = new Set(share_columns.map((column) => column.id));
    const candidate_column_ids = Array.isArray(candidate?.selected_column_ids)
      ? candidate.selected_column_ids
      : [];
    selected_column_ids = candidate
      ? candidate_column_ids.filter((id) => available_ids.has(id))
      : [];
    const available_task_ids = new Set(
      share_columns.flatMap((column) => column.tasks.map((task) => task.id)),
    );
    const candidate_task_ids = Array.isArray(candidate?.selected_task_ids)
      ? candidate.selected_task_ids
      : [];
    selected_task_ids = candidate
      ? candidate_task_ids.filter((id) => available_task_ids.has(id))
      : [];
    selected_labels = [...(candidate?.selected_labels ?? [])];
    if (candidate?.expires_at) {
      const expires = new Date(candidate.expires_at);
      expiration_enabled = true;
      expiration_date = new CalendarDate(
        expires.getFullYear(),
        expires.getMonth() + 1,
        expires.getDate(),
      );
    } else if (candidate) {
      expiration_enabled = false;
    } else {
      expiration_enabled = true;
      expiration_date = today(getLocalTimeZone()).add({ days: 30 });
    }
    task_search = "";
    reuse_link_open = false;
    requested_link = "";
    context_menu_share_id = null;
  }

  async function select_share(
    candidate: ManagedShare | null,
    editing_state = candidate === null,
  ): Promise<boolean> {
    const target_board_id =
      candidate?.board_id ??
      (owned_boards.some((board) => board.id === active_board_id)
        ? active_board_id
        : (owned_boards[0]?.id ?? null));
    if (!(await load_board_content(target_board_id))) {
      return false;
    }
    load_share(candidate, editing_state);
    return true;
  }

  async function select_new_share_board(board_id: number) {
    if (!owned_boards.some((board) => board.id === board_id)) {
      return;
    }
    if (!(await load_board_content(board_id))) {
      return;
    }
    selected_column_ids = [];
    selected_task_ids = [];
    selected_labels = [];
    task_search = "";
    title = `${board_name(board_id)} board`;
  }

  function cancel_editing() {
    if (!share) {
      return;
    }
    load_share(share);
  }

  $effect(() => {
    if (open && !initialized_for_open) {
      void select_share(managed_shares[0] ?? null);
      initialized_for_open = true;
    } else if (!open) {
      initialized_for_open = false;
    }
  });

  function set_column_selected(column_id: string, checked: boolean) {
    const task_ids =
      share_columns
        .find((column) => column.id === column_id)
        ?.tasks.map((task) => task.id) ?? [];
    selected_column_ids = checked
      ? Array.from(new Set([...selected_column_ids, column_id]))
      : selected_column_ids.filter((id) => id !== column_id);
    selected_task_ids = checked
      ? Array.from(new Set([...selected_task_ids, ...task_ids]))
      : selected_task_ids.filter((id) => !task_ids.includes(id));
  }

  function set_task_selected(
    column_id: string,
    task_id: string,
    checked: boolean,
  ) {
    if (checked) {
      selected_column_ids = Array.from(
        new Set([...selected_column_ids, column_id]),
      );
      selected_task_ids = Array.from(new Set([...selected_task_ids, task_id]));
    } else {
      selected_task_ids = selected_task_ids.filter((id) => id !== task_id);
    }
  }

  function expiration_value(): Date | null {
    if (!expiration_enabled) {
      return null;
    }
    return new Date(
      expiration_date.year,
      expiration_date.month - 1,
      expiration_date.day,
      23,
      59,
      59,
      999,
    );
  }

  const visible_columns = $derived.by(() => {
    const needle = task_search.trim().toLocaleLowerCase();
    if (!needle) {
      return share_columns;
    }
    return share_columns
      .map((column) => {
        if (column.name.toLocaleLowerCase().includes(needle)) {
          return column;
        }
        return {
          ...column,
          tasks: column.tasks.filter((task) =>
            [task.title, task.description, ...task.labels]
              .join(" ")
              .toLocaleLowerCase()
              .includes(needle),
          ),
        };
      })
      .filter((column) => column.tasks.length > 0);
  });
  const available_labels = $derived(
    Array.from(
      new Set(
        share_columns.flatMap((column) =>
          column.tasks.flatMap((task) => task.labels),
        ),
      ),
    ).sort(),
  );
  const effective_selection = $derived(
    resolve_share_selection(
      share_columns,
      selected_column_ids,
      selected_task_ids,
      selected_labels,
    ),
  );
  const visible_task_ids = $derived(
    visible_columns.flatMap((column) => column.tasks.map((task) => task.id)),
  );
  const all_visible_tasks_selected = $derived(
    visible_task_ids.length > 0 &&
      visible_task_ids.every((id) => selected_task_ids.includes(id)),
  );
  const no_visible_tasks_selected = $derived(
    visible_task_ids.every((id) => !selected_task_ids.includes(id)),
  );

  function set_visible_tasks_selected(include: boolean) {
    const task_ids = new Set(visible_task_ids);
    const column_ids = new Set(
      visible_columns
        .filter((column) => column.tasks.length > 0)
        .map((column) => column.id),
    );
    if (include) {
      selected_task_ids = Array.from(
        new Set([...selected_task_ids, ...task_ids]),
      );
      selected_column_ids = Array.from(
        new Set([...selected_column_ids, ...column_ids]),
      );
      return;
    }

    selected_task_ids = selected_task_ids.filter((id) => !task_ids.has(id));
    selected_column_ids = selected_column_ids.filter((column_id) => {
      if (!column_ids.has(column_id)) {
        return true;
      }
      return (
        share_columns
          .find((column) => column.id === column_id)
          ?.tasks.some((task) => selected_task_ids.includes(task.id)) ?? false
      );
    });
  }

  async function publish() {
    if (publishing) {
      return;
    }
    if (!owned_boards.some((board) => board.id === selected_board_id)) {
      toast.error(m.ui_only_boards_you_own_can_be_published());
      return;
    }
    publishing = true;
    try {
      const was_update = share !== null;
      const was_reused = !was_update && requested_link.trim() !== "";
      const snapshot = build_share_snapshot(
        share_columns,
        effective_selection.selected_column_ids,
        effective_selection.selected_task_ids,
        title,
      );
      const updated_share = await publish_share(
        snapshot,
        selected_column_ids,
        selected_task_ids,
        expiration_value(),
        // Updating an existing share keeps its URL; new shares receive an
        // independent capability ID and remain active alongside existing ones.
        share,
        selected_labels,
        requested_link,
        // Saving from this dialog is an explicit user action, so it may recover
        // an ID that was retired by a previous disable or an in-flight revoke.
        was_update,
        selected_board_id ?? undefined,
      );
      updated_share.board_id = selected_board_id ?? undefined;
      share = updated_share;
      requested_link = "";
      save_managed_share(
        updated_share,
        share_content_signature(
          share_columns,
          effective_selection.selected_column_ids,
          effective_selection.selected_task_ids,
          title,
        ),
      );
      load_share(updated_share, false);
      toast.success(
        was_update
          ? m.ui_published_view_updated()
          : was_reused
            ? m.ui_previous_web_address_reused()
            : m.ui_web_view_published(),
      );
    } catch (error) {
      logger.error("share.publish.failed", error);
      console.error(m.ui_couldn_t_publish_board_share(), error);
      toast.error(
        parseCommandError(error)
          ? translateCommandError(error)
          : m.ui_couldn_t_publish_the_web_view(),
      );
    } finally {
      publishing = false;
    }
  }

  async function copy_link(target: ManagedShare | null = share) {
    if (!target) {
      return;
    }
    try {
      await navigator.clipboard.writeText(target.url);
      toast.success(m.ui_web_address_copied());
    } catch (error) {
      logger.warn("share.link_copy.failed", error);
      toast.error(m.ui_couldn_t_copy_the_link());
    }
  }

  async function disable(target: ManagedShare | null = share) {
    if (!target || revoking) {
      return;
    }
    revoking = true;
    try {
      onRetireShare?.(target.id);
      await revoke_share(target);
      const disabled_share = { ...target, enabled: false };
      save_managed_share(disabled_share, "");
      if (share?.id === target.id) {
        load_share(disabled_share);
      }
      toast.success(
        m.ui_published_view_disabled_you_can_enable_it_again_later(),
      );
    } catch (error) {
      logger.error("share.disable.failed", error);
      console.error(m.ui_couldn_t_disable_board_share(), error);
      onShareRevokeError?.(target.id, error);
      toast.error(translateCommandError(error));
    } finally {
      revoking = false;
    }
  }

  async function enable(target: ManagedShare | null = share) {
    if (!target || publishing || is_managed_share_enabled(target)) {
      return;
    }
    publishing = true;
    try {
      // Context-menu actions can enable a link from another board without
      // selecting its row first. Always load that board before constructing a
      // snapshot so it cannot accidentally publish the current board's cards.
      if (!(await load_board_content(target.board_id ?? null))) {
        return;
      }
      const selection = resolve_share_selection(
        share_columns,
        target.selected_column_ids,
        target.selected_task_ids,
        target.selected_labels,
      );
      const snapshot = build_share_snapshot(
        share_columns,
        selection.selected_column_ids,
        selection.selected_task_ids,
        target.title,
      );
      const enabled_share = await publish_share(
        snapshot,
        target.selected_column_ids,
        target.selected_task_ids,
        target.expires_at ? new Date(target.expires_at) : null,
        target,
        target.selected_labels,
        null,
        true,
        target.board_id ?? selected_board_id ?? undefined,
      );
      save_managed_share(
        enabled_share,
        share_content_signature(
          share_columns,
          selection.selected_column_ids,
          selection.selected_task_ids,
          target.title,
        ),
      );
      if (share?.id === target.id) {
        load_share(enabled_share);
      }
      toast.success(m.ui_published_view_enabled());
    } catch (error) {
      logger.error("share.enable.failed", error);
      console.error(m.ui_couldn_t_enable_board_share(), error);
      toast.error(
        parseCommandError(error)
          ? translateCommandError(error)
          : m.ui_couldn_t_enable_the_published_view(),
      );
    } finally {
      publishing = false;
    }
  }

  async function delete_share() {
    if (!share || revoking) {
      return;
    }
    revoking = true;
    const deleted_id = share.id;
    try {
      onRetireShare?.(deleted_id);
      await revoke_share(share);
      forget_managed_share(deleted_id);
      const remaining = load_managed_shares().filter(
        (candidate) => candidate.id !== deleted_id,
      );
      share = null;
      await select_share(remaining[0] ?? null);
      delete_confirm_open = false;
      toast.success(m.ui_published_view_deleted());
    } catch (error) {
      logger.error("share.delete.failed", error);
      console.error(m.ui_couldn_t_delete_board_share(), error);
      onShareRevokeError?.(deleted_id, error);
      toast.error(translateCommandError(error));
    } finally {
      revoking = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    class="flex h-[min(90vh,48rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
  >
    <Dialog.Header class="shrink-0 px-6 pb-4 pt-6">
      <Dialog.Title>{m.board_publish()}</Dialog.Title>
      <Dialog.Description>
        {m.share_web_description()}
      </Dialog.Description>
    </Dialog.Header>

    <div
      class="grid min-h-0 flex-1 grid-rows-[auto_minmax(0,1fr)] overflow-hidden md:grid-cols-[16rem_minmax(0,1fr)] md:grid-rows-1"
    >
      <aside
        class="flex max-h-52 min-h-0 flex-col gap-3 overflow-y-auto border-b bg-muted/20 p-4 md:max-h-none md:border-b-0 md:border-r"
        aria-label={m.ui_published_web_views()}
      >
        <section class="grid gap-2.5" aria-labelledby="existing-shares-heading">
          <div class="flex items-center gap-2">
            <h3 id="existing-shares-heading" class="text-sm font-semibold">
              {m.ui_published_views()}
            </h3>
            <span
              class="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground"
            >
              {formatNumber(all_managed_shares.length)}
            </span>
          </div>
          {#if legacy_share_count > 0}
            <div
              class="grid gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-2 text-xs"
              role="status"
            >
              <p>
                {m.share_legacy_count({ count: legacy_share_count })}
              </p>
              {#each legacy_shares as legacy (legacy.id)}
                <div class="flex min-w-0 items-center justify-between gap-2">
                  <span class="min-w-0 flex-1 truncate">{legacy.title}</span>
                  <Button
                    class="shrink-0 whitespace-nowrap"
                    size="sm"
                    variant="outline"
                    disabled={active_board_id === null}
                    onclick={() =>
                      active_board_id !== null &&
                      rebind_managed_share(legacy.id, active_board_id)}
                    >{m.ui_assign_to_this_board()}</Button
                  >
                </div>
              {/each}
              <p class="text-muted-foreground">
                {m.ui_review_and_enable_it_after_assigning()}
              </p>
            </div>
          {/if}
          {#if all_managed_shares.length > 0}
            <div class="grid gap-1">
              {#each all_managed_shares as candidate (candidate.id)}
                {@const candidate_enabled = is_managed_share_enabled(candidate)}
                <ContextMenu.Root
                  open={context_menu_share_id === candidate.id}
                  onOpenChange={(menu_open) => {
                    if (menu_open) {
                      context_menu_share_id = candidate.id;
                    } else if (context_menu_share_id === candidate.id) {
                      context_menu_share_id = null;
                    }
                  }}
                >
                  <ContextMenu.Trigger>
                    {#snippet child({ props })}
                      <button
                        {...props}
                        type="button"
                        class={`grid w-full min-w-0 gap-1 rounded-md border px-3 py-2.5 text-left transition-colors ${
                          share?.id === candidate.id
                            ? "border-primary bg-primary/10 ring-1 ring-primary/30"
                            : "bg-card hover:border-primary/50 hover:bg-muted/40"
                        }`}
                        disabled={publishing || revoking}
                        onpointerdown={(event) => event.stopPropagation()}
                        onclick={(event) => {
                          event.stopPropagation();
                          void select_share(candidate);
                        }}
                        aria-pressed={share?.id === candidate.id}
                        title={`${candidate.title} — Right-click for actions`}
                      >
                        <span class="flex min-w-0 items-center gap-2">
                          <span class="truncate text-sm font-medium"
                            >{candidate.title}</span
                          >
                          <span
                            class={`size-1.5 shrink-0 rounded-full ${candidate_enabled ? "bg-success" : "bg-muted-foreground/50"}`}
                            aria-label={candidate_enabled
                              ? m.explorer_active()
                              : "Disabled"}
                          ></span>
                        </span>
                        <span class="text-xs text-muted-foreground">
                          <span
                            class={candidate_enabled
                              ? "text-success"
                              : undefined}
                          >
                            {candidate_enabled
                              ? m.explorer_active()
                              : m.ui_disabled()}
                          </span>
                          <span aria-hidden="true">·</span>
                          <span class="truncate"
                            >{board_name(candidate.board_id)}</span
                          >
                          {" · "}
                          {candidate.expires_at
                            ? m.share_expires_date({
                                date: share_date_formatter.format(
                                  new Date(candidate.expires_at),
                                ),
                              })
                            : m.ui_no_expiration()}
                        </span>
                      </button>
                    {/snippet}
                  </ContextMenu.Trigger>
                  <ContextMenu.Content class="w-56 rounded-lg p-1.5 shadow-lg">
                    <ContextMenu.Item
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      disabled={publishing || revoking}
                      onclick={() => void select_share(candidate, true)}
                    >
                      <PencilIcon class="size-4 text-muted-foreground" />
                      {m.ui_edit_settings()}
                    </ContextMenu.Item>
                    <ContextMenu.Item
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      onclick={() => void copy_link(candidate)}
                    >
                      <CopyIcon class="size-4 text-muted-foreground" />
                      {m.ui_copy_address()}
                    </ContextMenu.Item>
                    <ContextMenu.Separator class="my-1" />
                    <ContextMenu.Item
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      disabled={publishing || revoking}
                      onclick={() =>
                        candidate_enabled
                          ? void disable(candidate)
                          : void enable(candidate)}
                    >
                      <PowerIcon
                        class={candidate_enabled
                          ? "size-4 text-success"
                          : "size-4 text-muted-foreground"}
                      />
                      {candidate_enabled
                        ? m.ui_disable_view()
                        : m.ui_enable_view()}
                    </ContextMenu.Item>
                    <ContextMenu.Separator class="my-1" />
                    <ContextMenu.Item
                      variant="destructive"
                      class="h-9 gap-2.5 rounded-md px-2.5"
                      disabled={publishing || revoking}
                      onclick={() => {
                        void select_share(candidate).then((selected) => {
                          if (selected) {
                            delete_confirm_open = true;
                          }
                        });
                      }}
                    >
                      <Trash2Icon />
                      {m.ui_delete_view()}
                    </ContextMenu.Item>
                  </ContextMenu.Content>
                </ContextMenu.Root>
              {/each}
            </div>
          {:else}
            <div class="px-2 py-3 text-center">
              <p class="text-xs text-muted-foreground">
                {m.ui_no_published_views_yet()}
              </p>
            </div>
          {/if}
        </section>

        <section aria-labelledby="new-share-heading">
          <Button
            class={`w-full justify-start ${!share ? "ring-2 ring-primary/30" : ""}`}
            variant={share ? "outline" : "default"}
            disabled={publishing || revoking || !share}
            onclick={() => void select_share(null)}
            aria-current={!share ? "page" : undefined}
          >
            <PlusIcon />
            <span id="new-share-heading"
              >{share
                ? m.ui_new_published_view()
                : m.ui_creating_published_view()}</span
            >
          </Button>
        </section>
      </aside>

      <div
        class="grid min-h-0 content-start gap-5 overflow-y-auto px-5 py-5 sm:px-6"
      >
        {#if share}
          <section
            class={`grid content-start gap-4 rounded-xl border p-4 ${is_managed_share_enabled(share) ? "border-success/50 bg-success/5" : "border-border bg-muted/30"}`}
          >
            <div
              class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between"
            >
              <div class="flex min-w-0 items-start gap-3">
                <div
                  class={`flex size-9 shrink-0 items-center justify-center rounded-full ${is_managed_share_enabled(share) ? "bg-success/15 text-success" : "bg-muted text-muted-foreground"}`}
                >
                  <PowerIcon class="size-4" />
                </div>
                <div class="min-w-0">
                  <div class="flex flex-wrap items-center gap-2">
                    <h3 class="font-semibold">
                      {editing ? m.ui_edit_published_view() : share.title}
                    </h3>
                    <span
                      class={`inline-flex min-w-16 justify-center rounded-full px-2 py-0.5 text-xs font-medium ${is_managed_share_enabled(share) ? "bg-success/15 text-success" : "border border-border bg-muted text-muted-foreground"}`}
                    >
                      {is_managed_share_enabled(share)
                        ? m.explorer_active()
                        : m.ui_disabled()}
                    </span>
                  </div>
                  <p
                    class="mt-1 flex items-center gap-1.5 text-sm text-foreground"
                  >
                    <LayoutDashboardIcon
                      class="size-3.5 text-muted-foreground"
                      aria-hidden="true"
                    />
                    <span class="text-muted-foreground"
                      >{m.ui_source_board()}</span
                    >
                    <span class="font-medium">{board_name(share.board_id)}</span
                    >
                  </p>
                  <p class="mt-1 text-sm text-muted-foreground">
                    {is_managed_share_enabled(share)
                      ? m.ui_anyone_on_this_local_network_with_the_link_can_view_the_selected_content()
                      : m.ui_this_address_and_its_settings_are_saved_but_nobody_can_open_it()}
                  </p>
                </div>
              </div>
              {#if !editing}
                <div class="flex shrink-0 flex-wrap gap-2">
                  {#if is_managed_share_enabled(share)}
                    <Button
                      class="w-24"
                      variant="outline"
                      size="sm"
                      disabled={publishing || revoking}
                      onclick={() => void disable()}
                    >
                      <PowerIcon />
                      {revoking ? m.ui_disabling() : m.ui_disable()}
                    </Button>
                  {:else}
                    <Button
                      class="w-24"
                      size="sm"
                      disabled={publishing || revoking}
                      onclick={() => void enable()}
                    >
                      <PowerIcon />
                      {publishing ? m.ui_enabling() : m.ui_enable()}
                    </Button>
                  {/if}
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={publishing || revoking}
                    onclick={() => load_share(share, true)}
                  >
                    <PencilIcon />
                    {m.common_edit()}
                  </Button>
                </div>
              {/if}
            </div>
            <div class="grid gap-2">
              <span class="text-sm font-medium">{m.ui_web_address()}</span>
              <div class="flex gap-2">
                <Input
                  value={share.url}
                  readonly
                  aria-label={m.ui_web_address()}
                  class={!is_managed_share_enabled(share)
                    ? "text-muted-foreground"
                    : ""}
                />
                <Button
                  variant="outline"
                  size="icon"
                  onclick={() => void copy_link()}
                  aria-label={m.ui_copy_web_address()}
                  title={m.ui_copy_web_address()}
                >
                  <CopyIcon />
                </Button>
              </div>
              <p class="text-xs text-muted-foreground">
                {m.ui_last_published()}
                {share_datetime_formatter.format(new Date(share.updated_at))}
                {share.expires_at
                  ? ` · ${m.share_expires_date({ date: share_date_formatter.format(new Date(share.expires_at)) })}`
                  : " · No expiration"}
              </p>
            </div>
          </section>
        {/if}

        {#if share && !editing}
          <section
            class="grid gap-3"
            aria-label={m.ui_shared_content_settings()}
          >
            <div>
              <h3 class="text-sm font-semibold">{m.ui_share_settings()}</h3>
              <p class="text-xs text-muted-foreground">
                {m.ui_these_settings_control_what_visitors_can_see()}
              </p>
            </div>
            <div class="grid gap-4 rounded-lg border p-4 sm:grid-cols-2">
              <div>
                <div class="text-xs font-medium text-muted-foreground">
                  {m.ui_source_board()}
                </div>
                <p class="mt-1 text-sm font-medium">
                  {board_name(share.board_id)}
                </p>
              </div>
              <div>
                <div class="text-xs font-medium text-muted-foreground">
                  {m.ui_expiration()}
                </div>
                <p class="mt-1 text-sm font-medium">
                  {share.expires_at
                    ? share_date_formatter.format(new Date(share.expires_at))
                    : m.ui_no_expiration()}
                </p>
              </div>
              <div>
                <div class="text-xs font-medium text-muted-foreground">
                  {m.ui_shared_content()}
                </div>
                <p class="mt-1 text-sm font-medium">
                  {m.share_content_count({
                    columns: effective_selection.selected_column_ids.length,
                    tasks: effective_selection.selected_task_ids.length,
                  })}
                </p>
              </div>
              <div>
                <div class="text-xs font-medium text-muted-foreground">
                  {m.ui_automatic_labels()}
                </div>
                {#if selected_labels.length > 0}
                  <div class="mt-1 flex flex-wrap gap-1.5">
                    {#each selected_labels as label (label)}
                      <span class="rounded-md bg-muted px-2 py-0.5 text-xs"
                        >{label}</span
                      >
                    {/each}
                  </div>
                {:else}
                  <p class="mt-1 text-sm text-muted-foreground">
                    {m.ui_none()}
                  </p>
                {/if}
              </div>
            </div>
          </section>
        {/if}

        {#if !share || editing}
          {#if !share}
            <label class="grid gap-1.5 text-sm font-medium">
              {m.ui_board_to_share()}
              <select
                value={selected_board_id === null
                  ? ""
                  : String(selected_board_id)}
                disabled={publishing || loading_board_content}
                onchange={(event) =>
                  void select_new_share_board(
                    Number(event.currentTarget.value),
                  )}
                class="border-input bg-background focus-visible:border-ring focus-visible:ring-ring/50 h-10 w-full rounded-md border px-3 text-sm outline-none focus-visible:ring-[3px] disabled:opacity-50"
              >
                {#each owned_boards as target (target.id)}
                  <option value={String(target.id)}>{target.name}</option>
                {/each}
              </select>
              <span class="text-xs font-normal text-muted-foreground"
                >{m.ui_the_new_link_will_publish_content_from_this_board()}</span
              >
            </label>
          {:else}
            <p class="text-sm text-muted-foreground">
              {m.ui_sharing_from()}
              <span class="font-medium text-foreground"
                >{board_name(selected_board_id ?? undefined)}</span
              >
            </p>
          {/if}
          <label class="grid gap-1.5 text-sm font-medium">
            {m.ui_published_view_title()}
            <Input
              bind:value={title}
              maxlength={120}
              placeholder={m.ui_team_roadmap()}
            />
          </label>

          {#if !share}
            <div class="grid gap-2">
              <div>
                <div class="text-sm font-medium">{m.ui_web_address()}</div>
                <p class="text-xs text-muted-foreground">
                  {m.ui_create_a_new_address_or_restore_one_you_shared_before()}
                </p>
              </div>
              <div class="grid gap-2 sm:grid-cols-2">
                <button
                  type="button"
                  class={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors ${!reuse_link_open ? "border-primary bg-primary/5 ring-1 ring-primary/20" : "hover:bg-muted/40"}`}
                  onclick={() => {
                    reuse_link_open = false;
                    requested_link = "";
                  }}
                >
                  <CircleCheckIcon
                    class={`mt-0.5 size-4 shrink-0 ${!reuse_link_open ? "text-primary" : "text-muted-foreground"}`}
                  />
                  <span>
                    <span class="block text-sm font-medium"
                      >{m.ui_new_address()}</span
                    >
                    <span class="mt-0.5 block text-xs text-muted-foreground"
                      >{m.ui_best_for_a_new_audience()}</span
                    >
                  </span>
                </button>
                <button
                  type="button"
                  class={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors ${reuse_link_open ? "border-primary bg-primary/5 ring-1 ring-primary/20" : "hover:bg-muted/40"}`}
                  onclick={() => (reuse_link_open = true)}
                >
                  <RefreshCwIcon
                    class={`mt-0.5 size-4 shrink-0 ${reuse_link_open ? "text-primary" : "text-muted-foreground"}`}
                  />
                  <span>
                    <span class="block text-sm font-medium"
                      >{m.ui_reuse_previous_address()}</span
                    >
                    <span class="mt-0.5 block text-xs text-muted-foreground"
                      >{m.ui_reconnect_an_old_shared_url()}</span
                    >
                  </span>
                </button>
              </div>
              {#if reuse_link_open}
                <div
                  id="reuse-share-link-options"
                  class="grid gap-2 rounded-lg border bg-muted/20 p-3"
                >
                  <label for="previous-share-link" class="text-sm font-medium"
                    >{m.ui_previous_address_or_share_id()}</label
                  >
                  <Input
                    id="previous-share-link"
                    bind:value={requested_link}
                    autocomplete="off"
                    spellcheck={false}
                    placeholder={m.ui_paste_the_link_you_shared_before()}
                  />
                  <p class="text-xs text-muted-foreground">
                    {m.share_reuse_description()}
                  </p>
                </div>
              {/if}
            </div>
          {/if}

          <div class="grid gap-2">
            <div class="flex items-center gap-2">
              <Checkbox
                id="share-expiration-enabled"
                checked={expiration_enabled}
                onCheckedChange={(checked) => {
                  expiration_enabled = checked === true;
                }}
              />
              <label for="share-expiration-enabled" class="text-sm font-medium"
                >{m.ui_set_web_address_expiration()}</label
              >
            </div>
            {#if expiration_enabled}
              <Popover.Root bind:open={expiration_open}>
                <Popover.Trigger>
                  {#snippet child({ props })}
                    <Button
                      {...props}
                      variant="outline"
                      class="w-full justify-between font-normal"
                    >
                      <span class="flex items-center gap-2">
                        <CalendarDaysIcon class="size-4" />
                        {share_date_formatter.format(
                          expiration_date.toDate(getLocalTimeZone()),
                        )}
                      </span>
                      <ChevronDownIcon />
                    </Button>
                  {/snippet}
                </Popover.Trigger>
                <Popover.Content
                  class="w-auto overflow-hidden p-0"
                  align="start"
                >
                  <Calendar
                    type="single"
                    bind:value={expiration_date}
                    minValue={today(getLocalTimeZone())}
                    captionLayout="dropdown"
                    onValueChange={() => {
                      expiration_open = false;
                    }}
                  />
                </Popover.Content>
              </Popover.Root>
            {:else}
              <p class="text-xs text-muted-foreground">
                {m.ui_the_web_address_remains_active_until_you_disable_it()}
              </p>
            {/if}
          </div>

          <div class="grid gap-1.5">
            <div class="text-sm font-medium">
              {m.ui_automatically_include_labels()}
            </div>
            <TagInput
              bind:tags={selected_labels}
              suggestions={available_labels}
              placeholder={m.ui_add_label()}
            />
            <p class="text-xs text-muted-foreground">
              {m.share_labels_description()}
            </p>
          </div>

          <div class="grid gap-2">
            <div class="flex items-center justify-between">
              <div>
                <div class="text-sm font-medium">
                  {m.ui_content_to_include()}
                </div>
                <div class="text-xs text-muted-foreground">
                  {m.share_selected_count({
                    columns: effective_selection.selected_column_ids.length,
                    tasks: effective_selection.selected_task_ids.length,
                  })}
                </div>
              </div>
            </div>
            <div class="relative">
              <SearchIcon
                class="pointer-events-none absolute left-3 top-2.5 size-4 text-muted-foreground"
              />
              <Input
                class="pl-9"
                bind:value={task_search}
                placeholder={m.ui_search_columns_tasks_descriptions_or_labels()}
                aria-label={m.ui_search_shareable_tasks()}
              />
            </div>
            <div class="flex items-center justify-between gap-3">
              <span class="text-xs tabular-nums text-muted-foreground">
                {task_search.trim()
                  ? m.share_matching_tasks({ count: visible_task_ids.length })
                  : m.share_available_tasks({ count: visible_task_ids.length })}
              </span>
              <div class="flex gap-1">
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={visible_task_ids.length === 0 ||
                    all_visible_tasks_selected}
                  onclick={() => set_visible_tasks_selected(true)}
                >
                  {m.ui_select_all()}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={visible_task_ids.length === 0 ||
                    no_visible_tasks_selected}
                  onclick={() => set_visible_tasks_selected(false)}
                >
                  {m.ui_clear_all()}
                </Button>
              </div>
            </div>
            <div
              class="max-h-72 space-y-2 overflow-y-auto rounded-md border p-2"
            >
              {#each visible_columns as column (column.id)}
                {@const all_column_task_ids =
                  share_columns
                    .find((candidate) => candidate.id === column.id)
                    ?.tasks.map((task) => task.id) ?? []}
                {@const selected_in_column = all_column_task_ids.filter((id) =>
                  selected_task_ids.includes(id),
                ).length}
                <div class="rounded-md border bg-muted/20">
                  <label
                    class="flex cursor-pointer items-center gap-3 rounded-t-md px-3 py-2 hover:bg-muted/60"
                  >
                    <Checkbox
                      checked={selected_column_ids.includes(column.id) &&
                        selected_in_column === all_column_task_ids.length}
                      indeterminate={selected_column_ids.includes(column.id) &&
                        all_column_task_ids.length > 0 &&
                        selected_in_column < all_column_task_ids.length}
                      onCheckedChange={(checked) =>
                        set_column_selected(column.id, checked === true)}
                    />
                    <span class="min-w-0 flex-1 truncate text-sm font-medium"
                      >{column.name}</span
                    >
                    <span class="text-xs tabular-nums text-muted-foreground">
                      {selected_in_column}/{formatNumber(
                        all_column_task_ids.length,
                      )}
                    </span>
                  </label>
                  {#if column.tasks.length > 0}
                    <div class="border-t px-2 py-1">
                      {#each column.tasks as task (task.id)}
                        <label
                          class="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 hover:bg-muted/60"
                        >
                          <Checkbox
                            class="mt-0.5"
                            checked={selected_task_ids.includes(task.id)}
                            onCheckedChange={(checked) =>
                              set_task_selected(
                                column.id,
                                task.id,
                                checked === true,
                              )}
                          />
                          <span class="min-w-0 flex-1">
                            <span class="block truncate text-sm"
                              >{task.title || m.task_untitled()}</span
                            >
                            {#if task.labels.length > 0}
                              <span
                                class="block truncate text-xs text-muted-foreground"
                                >{task.labels.join(" · ")}</span
                              >
                            {/if}
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
          </div>
        {/if}
      </div>
    </div>

    <Dialog.Footer
      class="shrink-0 border-t bg-background px-6 py-4 sm:justify-end"
    >
      <div class="flex flex-wrap justify-end gap-2">
        {#if share}
          <Button
            variant="ghost"
            class="text-destructive hover:bg-destructive/10 hover:text-destructive"
            disabled={publishing || revoking}
            onclick={() => (delete_confirm_open = true)}
          >
            <Trash2Icon />
            {m.ui_delete_view()}
          </Button>
        {/if}
        {#if share && editing}
          <Button
            variant="outline"
            disabled={publishing || revoking}
            onclick={cancel_editing}>{m.common_cancel()}</Button
          >
          <Button
            disabled={publishing ||
              revoking ||
              effective_selection.selected_column_ids.length === 0}
            onclick={() => void publish()}
          >
            <RefreshCwIcon />
            {publishing ? m.common_saving() : m.ui_save_changes()}
          </Button>
        {:else if !share}
          <Button
            disabled={publishing ||
              revoking ||
              effective_selection.selected_column_ids.length === 0}
            onclick={() => void publish()}
          >
            {publishing
              ? m.ui_publishing()
              : requested_link.trim()
                ? m.ui_reuse_address()
                : m.ui_publish_web_view()}
          </Button>
        {/if}
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<AlertDialog.Root bind:open={delete_confirm_open}>
  <AlertDialog.Content class="sm:max-w-md">
    <AlertDialog.Header>
      <AlertDialog.Title>{m.ui_delete_this_published_view()}</AlertDialog.Title>
      <AlertDialog.Description>
        {m.share_delete_description()}
      </AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={revoking}
        >{m.ui_keep_view()}</AlertDialog.Cancel
      >
      <Button
        variant="destructive"
        disabled={revoking}
        onclick={() => void delete_share()}
      >
        <Trash2Icon />
        {revoking ? m.ui_deleting() : m.ui_delete_view()}
      </Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
