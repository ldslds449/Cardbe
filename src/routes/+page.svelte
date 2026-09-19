<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { getName, getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { ModeWatcher } from "mode-watcher";
  import { toast } from "svelte-sonner";
  import { Button } from "$lib/components/ui/button/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";

  import BugIcon from "@lucide/svelte/icons/bug";
  import RefreshCwIcon from "@lucide/svelte/icons/refresh-cw";
  import WifiIcon from "@lucide/svelte/icons/wifi";

  import {
    create_task,
    reset_task,
    clone_task,
    task_from_template,
    type Task,
  } from "./type/task.svelte";
  import BoardView from "./components/board/board_view.svelte";
  import ArchivePanel from "./components/archive/archive_panel.svelte";
  import ExpiredPanel from "./components/expire/expire_panel.svelte";
  import AddColumnDialog from "./components/dialog/add_column_dialog.svelte";
  import EditTaskDialog from "./components/dialog/edit_task_dialog.svelte";
  import ImportConfirmDialog from "./components/dialog/import_confirm_dialog.svelte";
  import TaskColumnDialog from "./components/dialog/task_column_dialog.svelte";
  import TaskTransferDialog from "./components/dialog/task_transfer_dialog.svelte";
  import TaskTemplateDialogs from "./components/dialog/task_template_dialogs.svelte";
  import ViewTaskDialog from "./components/dialog/view_task_dialog.svelte";
  import BoardShareDialog from "./components/dialog/board_share_dialog.svelte";
  import CalendarView from "./components/calendar/calendar_view.svelte";
  import type { CalendarViewMode } from "./components/calendar/calendar";
  import FocusView from "./components/focus/focus_view.svelte";
  import RecurringPanel from "./components/recurring/recurring_panel.svelte";
  import NotePanel from "./components/note/note_panel.svelte";
  import CardReferencePreview from "./components/card_reference_preview.svelte";
  import AppToolbar from "./components/workspace/app_toolbar.svelte";
  import WorkspaceTabs from "./components/workspace/workspace_tabs.svelte";
  import type { WorkspaceView } from "./components/workspace/workspace";
  import {
    markUpdateCheckSuccessful,
    shouldCheckForUpdate,
    UPDATE_STARTUP_DELAY_MS,
    updateCheckErrorMessage,
  } from "./utils/update";
  import { parse_portable_task, serialize_portable_task } from "./utils/task-transfer";
  import {
    build_share_snapshot,
    is_managed_share_enabled,
    publish_share,
    revoke_share,
    resolve_share_selection,
    save_managed_share,
    share_content_signature,
  } from "./share";
  import { managed_share_state, restore_managed_share_state } from "./share-state.svelte";

  import { board, type ImportCandidate } from "./board.svelte";

  // menubar state
  let task_expand_mode = $state(true);
  let active_view = $state<WorkspaceView>("focus");
  let selected_view = $state<WorkspaceView>("focus");
  let view_switch_sequence = 0;
  let search_text = $state("");
  let calendar_visible_date = $state(new Date());
  let calendar_view_mode = $state<CalendarViewMode>("month");
  let calendar_show_archived = $state(false);
  let calendar_show_recurring_previews = $state(true);
  let app_name = $state("");
  let app_name_promise: Promise<string> | undefined;
  let update_check_in_progress = $state(false);
  let startup_screen_dismissed = false;

  $effect(() => {
    if (startup_screen_dismissed || !board.column_fetch_finish) return;
    startup_screen_dismissed = true;
    // Let the workspace paint before the HTML-level startup card fades away.
    requestAnimationFrame(() => window.dispatchEvent(new Event("cardbe:workspace-ready")));
  });

  // archive panel
  let archive_open = $state(false);

  // expired panel
  let expired_open = $state(false);

  // recurring tasks panel
  let recurring_open = $state(false);

  // quick notes panel
  let note_open = $state(false);

  // column dialog
  let column_dialog_open = $state(false);
  let column_dialog_title_error = $state(false);
  let dialog_column_name = $state("");

  // task edit dialog
  let task_dialog_open = $state(false);
  let dialog_task = $state<Task>(create_task());
  let dialog_title = $state("");
  let dialog_button_text = $state("");
  let dialog_callback = $state<() => Promise<boolean>>(async () => false);
  let creating_task = $state(false);
  let dialog_template_id = $state("none");

  // task templates
  let templates_dialog_open = $state(false);
  let selected_template_id = $state("");
  let template_column_id = $state("");
  let template_selection_error = $state(false);
  let save_template_dialog_open = $state(false);
  let save_template_task_id = $state("");
  let save_template_name = $state("");
  let saving_template = $state(false);
  let template_preview_open = $state(false);
  let template_preview_task = $state<Task | null>(null);
  let editing_template_id = $state<number | null>(null);
  let editing_template_name = $state("");
  $effect(() => {
    if (!task_dialog_open) {
      editing_template_id = null;
      creating_task = false;
    }
  });

  // task details dialog
  let view_task_dialog_open = $state(false);
  let view_task = $state<Task | null>(null);
  let view_task_edit_callback = $state<(() => void) | undefined>(undefined);

  type CardReferencePreviewState = {
    task_id: string;
    left: number;
    top: number;
    show_below: boolean;
  };
  let card_reference_preview = $state<CardReferencePreviewState | null>(null);
  const card_reference_preview_content = $derived.by(() => {
    if (!card_reference_preview) return undefined;
    for (const column of board.columns) {
      const task = column.tasks.find(
        (candidate) => candidate.id === card_reference_preview?.task_id,
      );
      if (task) return { task, location: column.name };
    }
    const archive = board.archives.find(
      (candidate) => candidate.task.id === card_reference_preview?.task_id,
    );
    return archive ? { task: archive.task, location: "Archived" } : undefined;
  });

  function view_task_routine(task: Task, onEdit?: () => void) {
    view_task = task;
    view_task_edit_callback = onEdit;
    view_task_dialog_open = true;
  }

  // task column selection dialog
  let task_column_dialog_open = $state(false);
  let task_column_id = $state("");
  let task_column_error = $state(false);
  let pending_task_due_time = $state<Date | undefined>(undefined);
  let pending_import_task = $state<Task | null>(null);
  const task_column_items = $derived(
    board.columns.map((column) => ({ value: column.id, label: column.name })),
  );

  // import confirmation
  let import_confirm_open = $state(false);
  let import_candidate = $state<ImportCandidate | null>(null);
  let importing = $state(false);
  let show_importing = $state(false);

  // individual task sharing
  let task_share_dialog_open = $state(false);
  let task_import_dialog_open = $state(false);
  let task_share_text = $state("");
  let task_import_text = $state("");
  let task_import_error = $state("");
  let task_import_target_column_id = $state<string | null>(null);
  let task_import_target_column_name = $derived(
    board.columns.find((column) => column.id === task_import_target_column_id)?.name,
  );

  // read-only board sharing
  let board_share_dialog_open = $state(false);
  const share_time_formatter = new Intl.DateTimeFormat("en-US", {
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
  });
  const share_sync_summary = $derived.by(() => {
    const active_shares = managed_share_state.shares.filter(is_managed_share_enabled);
    const states = active_shares.map((share) => managed_share_state.sync_state(share.id));
    const failed = states.find((state) => state.status === "error");
    return {
      active_count: active_shares.length,
      failed,
      updating: states.some((state) => state.status === "pending" || state.status === "syncing"),
      latest_update: active_shares.reduce(
        (latest, share) => Math.max(latest, Date.parse(share.updated_at) || 0),
        0,
      ),
    };
  });

  $effect(() => {
    const shares = managed_share_state.shares.filter(is_managed_share_enabled);
    if (shares.length === 0 || !board.column_fetch_finish) return;

    const pending: Array<{ share: (typeof shares)[number]; signature: string }> = [];
    for (const share of shares) {
      try {
        const selection = resolve_share_selection(
          board.columns,
          share.selected_column_ids,
          share.selected_task_ids,
          share.selected_labels,
        );
        const signature = share_content_signature(
          board.columns,
          selection.selected_column_ids,
          selection.selected_task_ids,
          share.title,
        );
        if (signature !== managed_share_state.published_signatures[share.id]) {
          // Keep this effect independent from `sync_states`. Tracking the
          // state update would re-run it and reset the debounce timer.
          untrack(() => managed_share_state.set_sync_state(share.id, "pending"));
          pending.push({ share, signature });
        }
      } catch (error) {
        untrack(() => managed_share_state.set_sync_state(
          share.id,
          "error",
          error instanceof Error ? error.message : "Shared content is no longer available",
        ));
        void disable_stale_share(share);
      }
    }
    if (pending.length === 0) return;

    const timeout = window.setTimeout(() => {
      for (const item of pending) void sync_managed_share(item.share, item.signature);
    }, 700);
    return () => window.clearTimeout(timeout);
  });

  async function sync_managed_share(
    share: (typeof managed_share_state.shares)[number],
    signature: string,
  ) {
    if (!managed_share_state.shares.some((candidate) => candidate.id === share.id)) return;
    managed_share_state.set_sync_state(share.id, "syncing");
    try {
      const selection = resolve_share_selection(
        board.columns,
        share.selected_column_ids,
        share.selected_task_ids,
        share.selected_labels,
      );
      const snapshot = build_share_snapshot(
        board.columns,
        selection.selected_column_ids,
        selection.selected_task_ids,
        share.title,
      );
      const refreshed = await publish_share(
        snapshot,
        share.selected_column_ids,
        share.selected_task_ids,
        share.expires_at ? new Date(share.expires_at) : null,
        share,
        share.selected_labels,
      );
      if (managed_share_state.shares.some((candidate) => candidate.id === share.id)) {
        save_managed_share(refreshed, signature);
      }
    } catch (error) {
      if (!managed_share_state.shares.some((candidate) => candidate.id === share.id)) return;
      console.error("Couldn't update a board share", error);
      managed_share_state.set_sync_state(
        share.id,
        "error",
        error instanceof Error ? error.message : "Couldn't update the shared board",
      );
      // Keep the durable share metadata on transient failures (for example,
      // when its previous port is temporarily occupied). The user can retry
      // from the share dialog without losing the URL they already distributed.
    }
  }

  async function disable_stale_share(share: (typeof managed_share_state.shares)[number]) {
    try {
      await revoke_share(share);
      if (managed_share_state.shares.some((candidate) => candidate.id === share.id)) {
        managed_share_state.forget(share.id);
        toast.error(`Sharing stopped for “${share.title}” because its content could not be updated.`);
      }
    } catch {
      if (managed_share_state.shares.some((candidate) => candidate.id === share.id)) {
        managed_share_state.set_sync_state(
          share.id,
          "error",
          "Could not disable outdated share content. Close Cardbe to stop sharing.",
        );
      }
    }
  }

  onMount(() => {
    restore_managed_share_state();
    board.init();
    void load_app_name();

    const update_check_timeout = window.setTimeout(() => {
      void check_for_updates();
    }, UPDATE_STARTUP_DELAY_MS);

    const handle_shortcut = (event: KeyboardEvent) => {
      const target = event.target;
      const is_editing_text =
        target instanceof HTMLElement &&
        (target.matches("input, textarea, select, [contenteditable='true']") ||
          target.closest("[contenteditable='true']") !== null);
      if (
        is_editing_text ||
        task_dialog_open ||
        column_dialog_open ||
        task_column_dialog_open
      ) {
        return;
      }

      if (
        event.code === "KeyZ" &&
        (event.ctrlKey || event.metaKey) &&
        !event.shiftKey &&
        !event.altKey
      ) {
        event.preventDefault();
        void board.undo();
      } else if (
        event.code === "KeyT" &&
        event.ctrlKey &&
        event.shiftKey &&
        !event.altKey &&
        !event.metaKey
      ) {
        event.preventDefault();
        open_add_task_shortcut();
      } else if (
        event.code === "KeyC" &&
        event.ctrlKey &&
        event.shiftKey &&
        !event.altKey &&
        !event.metaKey
      ) {
        event.preventDefault();
        open_add_column_dialog();
      }
    };

    const handle_open_card = (event: Event) => {
      const task_id = (event as CustomEvent<{ taskId?: string }>).detail?.taskId;
      if (task_id) void open_card_reference(task_id);
    };
    const handle_show_card_preview = (event: Event) => {
      const detail = (event as CustomEvent<{
        taskId?: string;
        left?: number;
        top?: number;
        bottom?: number;
      }>).detail;
      if (
        !detail?.taskId ||
        detail.left === undefined ||
        detail.top === undefined ||
        detail.bottom === undefined
      ) return;

      const show_below = detail.top < 180;
      const preview_width = Math.min(288, window.innerWidth - 24);
      card_reference_preview = {
        task_id: detail.taskId,
        left: Math.max(12, Math.min(detail.left, window.innerWidth - preview_width - 12)),
        top: show_below ? detail.bottom + 8 : detail.top - 8,
        show_below,
      };
      if (!card_reference_preview_content && !board.archives_loaded) {
        void board.ensure_archives_loaded();
      }
    };
    const handle_hide_card_preview = () => {
      card_reference_preview = null;
    };

    document.addEventListener("keydown", handle_shortcut);
    window.addEventListener("cardbe:open-card", handle_open_card);
    window.addEventListener("cardbe:show-card-preview", handle_show_card_preview);
    window.addEventListener("cardbe:hide-card-preview", handle_hide_card_preview);
    const data_changed_listener = listen<{ kind?: string }>(
      "cardbe:data-changed",
      (event) => {
        if (event.payload.kind === "task") board.get_columns();
      },
    );
    return () => {
      window.clearTimeout(update_check_timeout);
      void data_changed_listener.then((unlisten) => unlisten());
      document.removeEventListener("keydown", handle_shortcut);
      window.removeEventListener("cardbe:open-card", handle_open_card);
      window.removeEventListener("cardbe:show-card-preview", handle_show_card_preview);
      window.removeEventListener("cardbe:hide-card-preview", handle_hide_card_preview);
    };
  });

  function load_app_name(): Promise<string> {
    app_name_promise ??= getName().then((name) => {
      app_name = name;
      return name;
    });
    return app_name_promise;
  }

  async function check_for_updates(manual = false) {
    if (update_check_in_progress) return;

    try {
      if (!manual && !shouldCheckForUpdate(window.localStorage)) return;
      update_check_in_progress = true;

      const [resolved_app_name, current_version] = await Promise.all([
        load_app_name(),
        getVersion(),
      ]);
      const update = await invoke<{ version: string; url: string } | null>("check_for_update", {
        currentVersion: current_version,
      });
      markUpdateCheckSuccessful(window.localStorage);
      if (!update) {
        if (manual) toast.success(`${resolved_app_name} is up to date`);
        return;
      }

      toast.info(`${resolved_app_name} ${update.version} is available`, {
        id: `app-update-${update.version}`,
        description: "Download the new version from GitHub Releases.",
        duration: Infinity,
        closeButton: true,
        action: {
          label: "View Release",
          onClick: () => {
            void invoke("open_external_url", { url: update.url }).catch(
              (error) => {
                console.error("Couldn't open the GitHub release:", error);
                toast.error("Couldn't open GitHub Releases");
              },
            );
          },
        },
        cancel: {
          label: "Later",
          onClick: () => {},
        },
      });
    } catch (error) {
      // Automatic update checks should never interrupt normal app usage.
      console.info("Couldn't check for app updates:", error);
      if (manual) {
        toast.error("Couldn't check for updates", {
          description: updateCheckErrorMessage(error),
          duration: 10_000,
          closeButton: true,
        });
      }
    } finally {
      update_check_in_progress = false;
    }
  }

  // ============ Dialog Handlers ============

  function open_add_column_dialog() {
    column_dialog_open = true;
    column_dialog_title_error = false;
    dialog_column_name = "";
  }

  function handle_add_column() {
    if (dialog_column_name.length > 0) {
      board.add_column(dialog_column_name);
      column_dialog_open = false;
    }
  }

  function edit_task_routine(col_idx: number, t_idx: number) {
    editing_template_id = null;
    creating_task = false;
    dialog_template_id = "none";
    const task_id = board.columns[col_idx].tasks[t_idx].id;
    dialog_title = "Edit Card";
    dialog_button_text = "Update";
    dialog_callback = () => board.update_task(task_id, dialog_task);
    dialog_task = clone_task(board.columns[col_idx].tasks[t_idx]);
    task_dialog_open = true;
  }

  function add_task_routine(col_idx: number, due_time?: Date) {
    editing_template_id = null;
    creating_task = true;
    dialog_template_id = "none";
    const column_id = board.columns[col_idx].id;
    dialog_title = "Add Card";
    dialog_button_text = "Add";
    dialog_callback = () => board.add_new_task(column_id, dialog_task);
    reset_task(dialog_task);
    dialog_task.due_time = due_time ? new Date(due_time) : undefined;
    task_dialog_open = true;
  }

  function open_add_task_shortcut() {
    pending_import_task = null;
    pending_task_due_time = undefined;
    if (board.columns.length === 0) {
      open_add_column_dialog();
      return;
    }
    task_column_id = "";
    task_column_error = false;
    task_column_dialog_open = true;
  }

  function open_add_task_for_date(date: Date) {
    pending_import_task = null;
    if (board.columns.length === 0) {
      open_add_column_dialog();
      return;
    }
    if (board.columns.length === 1) {
      add_task_routine(0, date);
      return;
    }

    pending_task_due_time = new Date(date);
    task_column_id = "";
    task_column_error = false;
    task_column_dialog_open = true;
  }

  function handle_task_column_select() {
    const column_idx = board.columns.findIndex((column) => column.id === task_column_id);
    if (column_idx === -1) {
      task_column_error = true;
      return;
    }
    task_column_dialog_open = false;
    if (pending_import_task) {
      open_imported_task(column_idx, pending_import_task);
      pending_import_task = null;
    } else {
      add_task_routine(column_idx, pending_task_due_time);
    }
    pending_task_due_time = undefined;
  }

  function open_imported_task(column_idx: number, task: Task) {
    editing_template_id = null;
    creating_task = true;
    dialog_template_id = "none";
    const column_id = board.columns[column_idx].id;
    dialog_title = "Import Task";
    dialog_button_text = "Import";
    dialog_task = clone_task(task);
    dialog_callback = () => board.add_new_task(column_id, dialog_task);
    task_dialog_open = true;
  }

  function open_templates_dialog() {
    selected_template_id = "";
    template_column_id = board.columns[0]?.id ?? "";
    template_selection_error = false;
    templates_dialog_open = true;
  }

  function open_save_template_dialog(task: Task) {
    save_template_task_id = task.id;
    save_template_name = task.title;
    save_template_dialog_open = true;
  }

  function preview_task_template(task: Task) {
    template_preview_task = clone_task(task);
    template_preview_open = true;
  }

  function edit_task_template(template_id: number) {
    const template = board.templates.find((candidate) => candidate.id === template_id);
    if (!template) return;
    editing_template_id = template.id;
    creating_task = false;
    dialog_template_id = "none";
    editing_template_name = template.name;
    dialog_title = "Edit Template";
    dialog_button_text = "Update Template";
    dialog_task = clone_task(template.task);
    dialog_callback = () =>
      board.update_task_template(template.id, editing_template_name, dialog_task);
    templates_dialog_open = false;
    task_dialog_open = true;
  }

  function apply_template_to_new_task(template_id: string) {
    if (!creating_task) return;

    const due_time = dialog_task.due_time
      ? new Date(dialog_task.due_time)
      : undefined;
    const template = board.templates.find(
      (candidate) => candidate.id.toString() === template_id,
    );

    if (template) {
      dialog_task = task_from_template(template.task);
    } else {
      reset_task(dialog_task);
    }
    dialog_task.due_time = due_time;
  }

  async function save_task_template() {
    const name = save_template_name.trim();
    if (!name || saving_template) return;
    saving_template = true;
    try {
      if (await board.save_task_template(save_template_task_id, name)) {
        save_template_dialog_open = false;
      }
    } finally {
      saving_template = false;
    }
  }

  function use_selected_template() {
    const template = board.templates.find(
      (candidate) => candidate.id.toString() === selected_template_id,
    );
    const column = board.columns.find((candidate) => candidate.id === template_column_id);
    if (!template || !column) {
      template_selection_error = true;
      return;
    }

    editing_template_id = null;
    creating_task = true;
    dialog_template_id = template.id.toString();
    dialog_title = "Add Card from Template";
    dialog_button_text = "Add";
    dialog_task = task_from_template(template.task);
    dialog_callback = () => board.add_new_task(column.id, dialog_task);
    templates_dialog_open = false;
    task_dialog_open = true;
  }

  async function prepare_import() {
    const candidate = await board.prepare_import_from_file();
    if (!candidate) return;
    import_candidate = candidate;
    import_confirm_open = true;
  }

  async function confirm_import() {
    if (!import_candidate || importing) return;
    importing = true;
    const importing_indicator_timeout = window.setTimeout(() => {
      show_importing = true;
    }, 300);
    try {
      if (await board.import_data(import_candidate.json_data)) {
        import_confirm_open = false;
        import_candidate = null;
      }
    } finally {
      window.clearTimeout(importing_indicator_timeout);
      show_importing = false;
      importing = false;
    }
  }

  function find_task_position(
    task_id: string,
  ): { column_idx: number; task_idx: number } | undefined {
    for (let column_idx = 0; column_idx < board.columns.length; column_idx++) {
      const task_idx = board.columns[column_idx].tasks.findIndex((task) => task.id === task_id);
      if (task_idx !== -1) {
        return { column_idx, task_idx };
      }
    }
    return undefined;
  }

  function edit_task_by_id(task: Task) {
    const position = find_task_position(task.id);
    if (position) edit_task_routine(position.column_idx, position.task_idx);
  }

  function view_task_by_id(task: Task) {
    view_task_routine(task, () => edit_task_by_id(task));
  }

  function view_archived_task(task: Task) {
    view_task_routine(task);
  }

  async function prepare_task_import(column_id?: string) {
    if (board.columns.length === 0) {
      toast.error("Create a column before importing a task");
      return;
    }
    if (column_id && !board.columns.some((column) => column.id === column_id)) {
      toast.error("Column no longer exists");
      return;
    }
    task_import_target_column_id = column_id ?? null;
    task_import_text = "";
    task_import_error = "";
    task_import_dialog_open = true;
  }

  async function import_shared_task() {
    let task: Task;
    try {
      task = await parse_portable_task(task_import_text);
    } catch (error) {
      task_import_error = error instanceof Error ? error.message : "Invalid task sharing text";
      return;
    }
    task_import_error = "";
    task_import_dialog_open = false;
    if (task_import_target_column_id) {
      const column_idx = board.columns.findIndex(
        (column) => column.id === task_import_target_column_id,
      );
      task_import_target_column_id = null;
      if (column_idx !== -1) {
        open_imported_task(column_idx, task);
        return;
      }
    }
    if (board.columns.length === 1) {
      open_imported_task(0, task);
      return;
    }
    pending_import_task = task;
    pending_task_due_time = undefined;
    task_column_id = "";
    task_column_error = false;
    task_column_dialog_open = true;
  }

  async function share_task(task: Task) {
    try {
      task_share_text = await serialize_portable_task(task);
      task_share_dialog_open = true;
    } catch (error) {
      console.log(error);
      toast.error("Couldn't prepare task sharing text");
    }
  }

  async function copy_task_share_text() {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(task_share_text);
      } else {
        const textarea = document.createElement("textarea");
        textarea.value = task_share_text;
        textarea.style.position = "fixed";
        textarea.style.opacity = "0";
        document.body.appendChild(textarea);
        textarea.select();
        const copied = document.execCommand("copy");
        textarea.remove();
        if (!copied) throw new Error("Copy command failed");
      }
      toast.success("Task sharing text copied");
    } catch (error) {
      console.log(error);
      toast.error("Couldn't copy text. Select it and copy manually.");
    }
  }

  async function open_card_reference(task_id: string) {
    const active_task = board.columns
      .flatMap((column) => column.tasks)
      .find((task) => task.id === task_id);
    if (active_task) {
      view_task_by_id(active_task);
      return;
    }

    await board.ensure_archives_loaded();
    const archived_task = board.archives
      .map((archive) => archive.task)
      .find((task) => task.id === task_id);
    if (archived_task) {
      view_archived_task(archived_task);
      return;
    }

    toast.error("Referenced card no longer exists");
  }

  function stop_task_recurrence(task: Task) {
    const updated = clone_task(task);
    updated.recurrence = undefined;
    void board.update_task(task.id, updated);
  }

  function reschedule_task(task: Task, due_time: Date) {
    const updated = clone_task(task);
    updated.due_time = new Date(due_time);
    void board.update_task(task.id, updated);
  }

  function switch_view(view: WorkspaceView) {
    selected_view = view;
    if (view === "calendar") void board.ensure_archives_loaded();
    const sequence = ++view_switch_sequence;

    // Paint the control feedback first, then render the heavier view content.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (sequence === view_switch_sequence) active_view = view;
      });
    });
  }
</script>

<div class="flex h-screen min-h-0 flex-col bg-background">
  <ModeWatcher />

  {#if board.column_fetch_error}
    <Empty.Root class="w-full">
      <Empty.Header>
        <Empty.Media variant="icon">
          <BugIcon />
        </Empty.Media>
        <Empty.Title>Error</Empty.Title>
        <Empty.Description>An error occurred during loading</Empty.Description>
      </Empty.Header>
      <Empty.Content>
        <Button onclick={() => board.get_columns()}>Retry</Button>
      </Empty.Content>
    </Empty.Root>
  {:else if !board.column_fetch_finish}
    <Empty.Root class="w-full">
      <Empty.Header>
        <Empty.Media variant="icon">
          <Spinner />
        </Empty.Media>
        <Empty.Title>Loading Data</Empty.Title>
        <Empty.Description>Please wait for a while...</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <AppToolbar
      {app_name}
      bind:search_text
      bind:archive_open
      bind:expired_open
      bind:recurring_open
      bind:note_open
      bind:task_expand_mode
      {selected_view}
      {update_check_in_progress}
      onPrepareImport={prepare_import}
      onPrepareTaskImport={prepare_task_import}
      onOpenBoardShare={() => {
        board_share_dialog_open = true;
      }}
      onOpenTaskTemplates={open_templates_dialog}
      onAddTask={open_add_task_shortcut}
      onAddColumn={open_add_column_dialog}
      onCheckForUpdates={check_for_updates}
    />

    <WorkspaceTabs {selected_view} onSwitchView={switch_view} />

    {#if share_sync_summary.active_count > 0}
      <button
        type="button"
        class={`flex h-8 shrink-0 items-center justify-between gap-3 border-b px-5 text-left text-xs transition-colors ${
          share_sync_summary.failed
            ? "bg-destructive/10 text-destructive hover:bg-destructive/15"
            : "bg-muted/40 text-muted-foreground hover:bg-muted/70"
        }`}
        onclick={() => {
          board_share_dialog_open = true;
        }}
        aria-label="Open active board share"
      >
        <span class="flex min-w-0 items-center gap-2 font-medium">
          {#if share_sync_summary.failed}
            <WifiIcon class="size-3.5 shrink-0" />
            Share sync needs attention
          {:else if share_sync_summary.updating}
            <RefreshCwIcon class="size-3.5 shrink-0 animate-spin" />
            Updating shared boards…
          {:else}
            <WifiIcon class="size-3.5 shrink-0" />
            Sharing {share_sync_summary.active_count} {share_sync_summary.active_count === 1 ? "board" : "boards"} on LAN
          {/if}
        </span>
        <span class="truncate opacity-80">
          {share_sync_summary.failed
            ? `Sync failed: ${share_sync_summary.failed.error}`
            : `Last synced ${share_time_formatter.format(new Date(share_sync_summary.latest_update))}`}
        </span>
      </button>
    {/if}

    <ArchivePanel
      bind:open={archive_open}
      archives={board.archives}
      loading={board.archives_loading}
      columns={board.columns}
      onUnarchive={board.unarchive_task.bind(board)}
    ></ArchivePanel>

    <ExpiredPanel
      bind:open={expired_open}
      expired_tasks={board.expired_tasks}
    ></ExpiredPanel>

    <RecurringPanel
      bind:open={recurring_open}
      columns={board.columns}
      onViewTask={view_task_by_id}
      onEditTask={edit_task_by_id}
      onStopRepeat={stop_task_recurrence}
    />

    <AddColumnDialog
      bind:open={column_dialog_open}
      bind:name={dialog_column_name}
      bind:title_error={column_dialog_title_error}
      onSubmit={handle_add_column}
    />

    <NotePanel bind:open={note_open} />

    <BoardShareDialog
      bind:open={board_share_dialog_open}
      columns={board.columns}
      default_title={app_name}
    />

    <TaskColumnDialog
      bind:open={task_column_dialog_open}
      bind:selected_column_id={task_column_id}
      bind:selection_error={task_column_error}
      column_items={task_column_items}
      onSubmit={handle_task_column_select}
      title={pending_import_task ? "Import Task" : "Add Card"}
      description={pending_import_task
        ? "Select the column where this task should be imported."
        : "Select the column for the new card."}
      submit_label={pending_import_task ? "Import" : "Continue"}
    />

    <TaskTransferDialog
      bind:export_open={task_share_dialog_open}
      bind:import_open={task_import_dialog_open}
      share_text={task_share_text}
      bind:import_text={task_import_text}
      import_error={task_import_error}
      import_target_column={task_import_target_column_name}
      onCopy={copy_task_share_text}
      onImport={import_shared_task}
    />

    <TaskTemplateDialogs
      bind:open={templates_dialog_open}
      bind:save_open={save_template_dialog_open}
      bind:selected_template_id
      bind:template_column_id
      bind:selection_error={template_selection_error}
      bind:save_template_name
      {saving_template}
      templates={board.templates}
      column_items={task_column_items}
      onUseTemplate={use_selected_template}
      onPreviewTemplate={preview_task_template}
      onEditTemplate={edit_task_template}
      onDeleteTemplate={(template_id) => board.delete_task_template(template_id)}
      onSaveTemplate={save_task_template}
    />

    <EditTaskDialog
      bind:open={task_dialog_open}
      bind:task={dialog_task}
      label_suggestions={board.labels}
      card_reference_options={board.columns.flatMap((column) =>
        column.tasks.map((task) => ({ task, column_name: column.name }))) }
      {dialog_title}
      submit_button_text={dialog_button_text}
      auto_focus_title={creating_task}
      show_template_name={editing_template_id !== null}
      bind:template_name={editing_template_name}
      show_template_picker={creating_task}
      templates={board.templates}
      bind:selected_template_id={dialog_template_id}
      on_template_change={apply_template_to_new_task}
      dialog_done_callback={async () => {
        if (await dialog_callback()) {
          task_dialog_open = false;
          editing_template_id = null;
          creating_task = false;
        }
      }}
    ></EditTaskDialog>

    <ViewTaskDialog
      bind:open={view_task_dialog_open}
      task={view_task}
      onEdit={view_task_edit_callback}
    />

    <ViewTaskDialog
      bind:open={template_preview_open}
      task={template_preview_task}
    />

    <ImportConfirmDialog
      bind:open={import_confirm_open}
      candidate={import_candidate}
      {importing}
      {show_importing}
      onConfirm={confirm_import}
    />

    <div class="flex-grow min-h-0">
      <ScrollArea
        orientation="both"
        class="h-full rounded-md p-5"
        scrollbarXClasses="h-4"
        scrollbarYClasses="w-4"
      >
        {#if active_view === "focus"}
          <FocusView
            columns={board.columns}
            {search_text}
            onViewTask={view_task_by_id}
            onEditTask={edit_task_by_id}
            onDuplicateTask={(task) => board.duplicate_task(task.id)}
            onSaveAsTemplate={open_save_template_dialog}
            onExportTask={share_task}
            onAddTask={open_add_task_for_date}
            onArchiveTask={(task) => board.archive_task(task.id)}
            onDeleteTask={(task) => board.delete_task(task.id)}
          />
        {:else if active_view === "calendar"}
          <CalendarView
            columns={board.columns}
            archives={board.archives}
            {search_text}
            bind:visible_date={calendar_visible_date}
            bind:view_mode={calendar_view_mode}
            bind:show_archived={calendar_show_archived}
            bind:show_recurring_previews={calendar_show_recurring_previews}
            onViewTask={view_task_by_id}
            onViewArchivedTask={view_archived_task}
            onEditTask={edit_task_by_id}
            onDuplicateTask={(task) => board.duplicate_task(task.id)}
            onSaveAsTemplate={open_save_template_dialog}
            onExportTask={share_task}
            onAddTask={open_add_task_for_date}
            onArchiveTask={(task) => board.archive_task(task.id)}
            onUnarchiveTask={(column_id, task_id) =>
              board.unarchive_task(column_id, task_id)}
            onDeleteTask={(task) => board.delete_task(task.id)}
            onRescheduleTask={reschedule_task}
          />
        {:else}
          <BoardView
            {search_text}
            {task_expand_mode}
            onAddColumn={open_add_column_dialog}
            onAddTask={add_task_routine}
            onImportTask={(column_id) => void prepare_task_import(column_id)}
            onViewTask={(task, column_idx, task_idx) =>
              view_task_routine(task, () => edit_task_routine(column_idx, task_idx))}
            onEditTask={edit_task_routine}
            onSaveAsTemplate={open_save_template_dialog}
            onExportTask={share_task}
          />
        {/if}
      </ScrollArea>
    </div>

    {#if card_reference_preview}
      <CardReferencePreview
        task={card_reference_preview_content?.task}
        location={card_reference_preview_content?.location}
        left={card_reference_preview.left}
        top={card_reference_preview.top}
        show_below={card_reference_preview.show_below}
      />
    {/if}
  {/if}
</div>
