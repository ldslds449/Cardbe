<script lang="ts">
  import { confirmUnsavedChanges } from "$lib/unsaved-changes";
  import { translateCommandError } from "$lib/command-errors";
  import { getLocale } from "$lib/i18n";
  import { m } from "$lib/paraglide/messages.js";
  import { logger } from "$lib/logger";
  import { restore_floating_tasks } from "$lib/floating-tasks";
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { toast } from "svelte-sonner";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import * as Empty from "$lib/components/ui/empty/index.js";

  import BugIcon from "@lucide/svelte/icons/bug";

  import {
    create_task,
    reset_task,
    clone_task,
    task_from_template,
    type Task,
  } from "./type/task.svelte";
  import {
    deserialize_column,
    type ColumnSerialized,
  } from "./type/column.svelte";
  import { IrohDeviceStatus, type IrohInvite } from "./type/iroh-share";
  import BoardView from "./components/board/board_view.svelte";
  import AllTaskExplorer from "./components/task/all_task_explorer.svelte";
  import AddColumnDialog from "./components/dialog/add_column_dialog.svelte";
  import EditTaskDialog from "./components/dialog/edit_task_dialog.svelte";
  import ImportConfirmDialog from "./components/dialog/import_confirm_dialog.svelte";
  import TaskColumnDialog from "./components/dialog/task_column_dialog.svelte";
  import TaskTransferDialog from "./components/dialog/task_transfer_dialog.svelte";
  import TaskTemplateDialogs from "./components/dialog/task_template_dialogs.svelte";
  import ViewTaskDialog from "./components/dialog/view_task_dialog.svelte";
  import BoardShareDialog from "./components/dialog/board_share_dialog.svelte";
  import IrohShareDialog from "./components/dialog/iroh_share_dialog.svelte";
  import FlowView from "./components/flow/flow_view.svelte";
  import CalendarView from "./components/calendar/calendar_view.svelte";
  import StatisticsView from "./components/statistics/statistics_view.svelte";
  import type { CalendarViewMode } from "./components/calendar/calendar";
  import GanttView from "./components/gantt/gantt_view.svelte";
  import type { GanttScale } from "./components/gantt/gantt";
  import type { TaskDates } from "./utils/task-date-range";
  import TodayView from "./components/today/today_view.svelte";
  import NotePanel from "./components/note/note_panel.svelte";
  import CardReferencePreview from "./components/card_reference_preview.svelte";
  import IrohRemovalNotice from "./components/share/iroh_removal_notice.svelte";
  import StartupErrorScreen from "./components/workspace/startup_error_screen.svelte";
  import AppToolbar from "./components/workspace/app_toolbar.svelte";
  import BoardSidebar from "./components/workspace/board_sidebar.svelte";
  import WorkspaceTabs from "./components/workspace/workspace_tabs.svelte";
  import type { WorkspaceView } from "./components/workspace/workspace";
  import {
    UPDATE_POLL_INTERVAL_MS,
    UPDATE_STARTUP_DELAY_MS,
  } from "./utils/update";
  import {
    parse_portable_task,
    serialize_portable_task,
  } from "./utils/task-transfer";
  import { revoke_managed_shares } from "./share";
  import {
    managed_share_state,
    restore_managed_share_state,
  } from "./share-state.svelte";
  import { ManagedShareSync } from "./share-sync.svelte";
  import { UpdateChecker } from "./utils/update-checker.svelte";

  import { board, type ImportCandidate } from "./board.svelte";

  // menubar state
  let active_view = $state<WorkspaceView>("today");
  let selected_view = $state<WorkspaceView>("today");
  $effect(() => {
    if (
      active_view === "statistics" &&
      board.active_board_id !== null &&
      board.column_fetch_finish
    ) {
      untrack(() => void board.ensure_archives_loaded());
    }
  });
  let view_switch_sequence = 0;
  let search_text = $state("");
  let calendar_visible_date = $state(new Date());
  let calendar_view_mode = $state<CalendarViewMode>("month");
  let calendar_show_archived = $state(false);
  let calendar_show_recurring_previews = $state(true);
  let gantt_scale = $state<GanttScale>("day");
  let gantt_show_archived = $state(false);
  let startup_error = $state.raw<unknown>(null);
  let startup_check_finished = $state(false);
  const updates = new UpdateChecker();
  let startup_screen_dismissed = false;
  let board_panel_open = $state(false);
  const active_board_summary = $derived(
    board.boards.find((item) => item.id === board.active_board_id),
  );
  const active_board_read_only = $derived(!board.can_edit);

  $effect(() => {
    void board.columns;
    const text = search_text;
    // Rerun only for board or query changes, not for the search state it sets.
    untrack(() => board.search_tasks(text));
  });

  $effect(() => {
    if (startup_screen_dismissed || !board.column_fetch_finish) {
      return;
    }
    startup_screen_dismissed = true;
    // Let the workspace paint before the HTML-level startup card fades away.
    requestAnimationFrame(() =>
      window.dispatchEvent(new Event("cardbe:workspace-ready")),
    );
  });

  // all-task explorer
  let all_task_open = $state(false);

  // quick notes panel
  let note_open = $state(false);
  let requested_note = $state<{ id: number } | null>(null);
  let note_revision = $state(0);

  // column dialog
  let column_dialog_open = $state(false);
  let column_dialog_title_error = $state(false);
  let dialog_column_name = $state("");

  // task edit dialog
  let task_dialog_open = $state(false);
  let dialog_task = $state<Task>(create_task());
  let dialog_title = $state(m.task_add_card);
  let dialog_button_text = $state(m.common_add);
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
    if (!card_reference_preview) {
      return undefined;
    }
    for (const column of board.columns) {
      const task = column.tasks.find(
        (candidate) => candidate.id === card_reference_preview?.task_id,
      );
      if (task) {
        return { task, location: column.name };
      }
    }
    const archive = board.archives.find(
      (candidate) => candidate.task.id === card_reference_preview?.task_id,
    );
    return archive
      ? { task: archive.task, location: m.explorer_archived() }
      : undefined;
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
  let pending_task_created: ((id: string) => void) | undefined;
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
    board.columns.find((column) => column.id === task_import_target_column_id)
      ?.name,
  );

  // read-only board sharing
  let board_share_dialog_open = $state(false);
  let iroh_share_dialog_open = $state(false);
  let iroh_show_requests = $state(false);
  let iroh_show_received = $state(false);
  const removed_access_count = $derived(
    board.boards.filter(
      (item) =>
        item.shared_role !== "owner" && board.iroh_access_removed[item.id],
    ).length,
  );
  let pending_device_count = $state(0);
  async function refresh_device_requests() {
    try {
      const invites = await invoke<IrohInvite[]>("list_iroh_invites");
      pending_device_count = invites.reduce(
        (count, invite) =>
          count +
          invite.devices.filter(
            (device) => device.status === IrohDeviceStatus.Pending,
          ).length,
        0,
      );
    } catch (error) {
      logger.warn("iroh.device_requests_refresh.failed", error);
      pending_device_count = 0;
    }
  }
  const share_time_formatter = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    }),
  );
  const share_sync = new ManagedShareSync();
  share_sync.track_active_board();

  function reset_board_scoped_ui() {
    search_text = "";
    task_dialog_open = false;
    view_task_dialog_open = false;
    column_dialog_open = false;
    templates_dialog_open = false;
    save_template_dialog_open = false;
    template_preview_open = false;
    import_confirm_open = false;
    task_column_dialog_open = false;
    board_share_dialog_open = false;
    card_reference_preview = null;
    view_task = null;
    template_preview_task = null;
    selected_template_id = "";
    template_column_id = "";
    task_column_id = "";
    pending_import_task = null;
    pending_task_due_time = undefined;
    import_candidate = null;
    pending_task_created = undefined;
    editing_template_id = null;
    task_import_target_column_id = null;
  }

  let previous_board_id = board.active_board_id;
  $effect(() => {
    const current_board_id = board.active_board_id;
    if (current_board_id !== previous_board_id) {
      previous_board_id = current_board_id;
      untrack(reset_board_scoped_ui);
    }
  });

  async function switch_board(id: number) {
    if (id === board.active_board_id) {
      board_panel_open = false;
      return;
    }
    if (!(await confirmUnsavedChanges())) {
      return;
    }
    try {
      await share_sync.flush_active_board();
      if (await board.switch_board(id)) {
        reset_board_scoped_ui();
      }
    } catch (error) {
      logger.error("board.switch_share_sync.failed", error);
      toast.error(translateCommandError(error));
    }
  }

  async function delete_board(id: number): Promise<boolean> {
    try {
      for (const share of managed_share_state.shares) {
        if (share.board_id === id) {
          share_sync.retire(share.id);
        }
      }
      const forget_shares = await revoke_managed_shares(
        (share) => share.board_id === id,
        "board_deleted",
      );
      const deleted = await board.delete_board(id);
      if (deleted) {
        forget_shares();
        reset_board_scoped_ui();
      }
      return deleted;
    } catch (error) {
      logger.error("board.delete_share_revoke.failed", error);
      // Revoked links remain configured but disabled until deletion succeeds.
      for (const share of managed_share_state.shares) {
        if (share.board_id === id) {
          managed_share_state.set_sync_state(
            share.id,
            "error",
            m.share_revoke_delete(),
          );
        }
      }
      toast.error(m.ui_couldn_t_revoke_this_board_s_share_links());
      return false;
    }
  }

  async function create_board(name: string): Promise<boolean> {
    const created = await board.create_board(name);
    if (created) {
      reset_board_scoped_ui();
    }
    return created;
  }

  async function restore_everything() {
    let forget_shares: (() => void) | undefined;
    try {
      if (
        await board.import_all_boards_from_file(true, async () => {
          for (const share of managed_share_state.shares) {
            share_sync.retire(share.id);
          }
          forget_shares = await revoke_managed_shares(
            () => true,
            "backup_restore",
          );
        })
      ) {
        forget_shares?.();
        reset_board_scoped_ui();
      }
    } catch (error) {
      logger.error("backup.restore_share_revoke.failed", error);
      // Keep configurations on failure, including links already revoked.
      for (const share of managed_share_state.shares) {
        managed_share_state.set_sync_state(
          share.id,
          "error",
          m.share_revoke_restore(),
        );
      }
      toast.error(m.share_restore_stopped());
    }
  }

  onMount(() => {
    let disposed = false;
    let device_request_timer: number | undefined;
    void invoke<unknown>("get_startup_error")
      .then((error) => {
        if (disposed) {
          return;
        }
        startup_check_finished = true;
        if (error) {
          startup_error = error;
          requestAnimationFrame(() =>
            window.dispatchEvent(new Event("cardbe:workspace-ready")),
          );
          return;
        }
        void refresh_device_requests();
        device_request_timer = window.setInterval(
          () => void refresh_device_requests(),
          5000,
        );
        void board.init().then(async () => {
          if (disposed) {
            return;
          }
          void restore_floating_tasks();
          restore_managed_share_state(
            board.boards.length === 1 ? board.boards[0].id : undefined,
          );
          if (managed_share_state.storage_error) {
            toast.error(
              managed_share_state.storage_error === "read"
                ? m.share_settings_read_failed()
                : m.share_settings_write_failed(),
            );
            return;
          }
          await share_sync.restore_all();
        });
      })
      .catch((error) => {
        if (disposed) {
          return;
        }
        startup_error = error;
        startup_check_finished = true;
        requestAnimationFrame(() =>
          window.dispatchEvent(new Event("cardbe:workspace-ready")),
        );
      });
    void updates.load_app_name();

    const update_check_timeout = window.setTimeout(() => {
      void updates.check();
    }, UPDATE_STARTUP_DELAY_MS);
    const update_check_interval = window.setInterval(() => {
      void updates.check();
    }, UPDATE_POLL_INTERVAL_MS);

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
      const task_id = (event as CustomEvent<{ taskId?: string }>).detail
        ?.taskId;
      if (task_id) {
        void open_card_reference(task_id);
      }
    };
    const handle_show_card_preview = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          taskId?: string;
          left?: number;
          top?: number;
          bottom?: number;
        }>
      ).detail;
      if (
        !detail?.taskId ||
        detail.left === undefined ||
        detail.top === undefined ||
        detail.bottom === undefined
      ) {
        return;
      }

      const show_below = detail.top < 180;
      const preview_width = Math.min(288, window.innerWidth - 24);
      card_reference_preview = {
        task_id: detail.taskId,
        left: Math.max(
          12,
          Math.min(detail.left, window.innerWidth - preview_width - 12),
        ),
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
    window.addEventListener(
      "cardbe:show-card-preview",
      handle_show_card_preview,
    );
    window.addEventListener(
      "cardbe:hide-card-preview",
      handle_hide_card_preview,
    );
    const data_changed_listener = listen<{ kind?: string; boardId?: number }>(
      "cardbe:data-changed",
      (event) => {
        if (
          event.payload.kind !== "task" ||
          !Number.isInteger(event.payload.boardId)
        ) {
          return;
        }
        const changed_board_id = event.payload.boardId!;
        if (changed_board_id === board.active_board_id) {
          board.get_columns();
          return;
        }
        void invoke<ColumnSerialized[]>("get_board_columns", {
          boardId: changed_board_id,
        })
          .then((columns) =>
            share_sync.sync_board(
              changed_board_id,
              columns.map(deserialize_column),
            ),
          )
          .catch((error) => {
            logger.error("share.quick_add_sync.failed", error);
          });
      },
    );
    const open_task_listener = listen<{ boardId?: number; taskId?: number }>(
      "cardbe:open-task",
      async (event) => {
        const { boardId, taskId } = event.payload;
        if (!Number.isInteger(boardId) || !Number.isInteger(taskId)) {
          return;
        }
        if (!(await board.switch_board(boardId!))) {
          return;
        }
        const task = await board.get_expired_task_detail(`task_${taskId}`);
        if (task) {
          view_task_by_id(task);
        } else {
          toast.error(m.task_missing());
        }
      },
    );
    const iroh_remote_push_listener = listen<{
      board_id: number;
      revision: number;
    }>("cardbe:iroh-remote-push", (event) => {
      const { board_id, revision } = event.payload;
      if (Number.isInteger(board_id) && Number.isInteger(revision)) {
        board.handle_iroh_remote_push(board_id, revision);
      }
    });
    return () => {
      disposed = true;
      if (device_request_timer !== undefined) {
        window.clearInterval(device_request_timer);
      }
      window.clearTimeout(update_check_timeout);
      window.clearInterval(update_check_interval);
      void data_changed_listener.then((unlisten) => unlisten());
      void iroh_remote_push_listener.then((unlisten) => unlisten());
      void open_task_listener.then((unlisten) => unlisten());
      document.removeEventListener("keydown", handle_shortcut);
      window.removeEventListener("cardbe:open-card", handle_open_card);
      window.removeEventListener(
        "cardbe:show-card-preview",
        handle_show_card_preview,
      );
      window.removeEventListener(
        "cardbe:hide-card-preview",
        handle_hide_card_preview,
      );
    };
  });

  // ============ Dialog Handlers ============

  function open_add_column_dialog() {
    if (active_board_read_only) {
      return;
    }
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
    if (active_board_read_only) {
      return;
    }
    editing_template_id = null;
    creating_task = false;
    dialog_template_id = "none";
    const task_id = board.columns[col_idx].tasks[t_idx].id;
    dialog_title = m.task_edit;
    dialog_button_text = m.common_update;
    dialog_callback = () => board.update_task(task_id, dialog_task);
    dialog_task = clone_task(board.columns[col_idx].tasks[t_idx]);
    task_dialog_open = true;
  }

  function add_task_routine(
    col_idx: number,
    due_time?: Date,
    onCreated?: (id: string) => void,
  ) {
    if (active_board_read_only) {
      return;
    }
    editing_template_id = null;
    creating_task = true;
    dialog_template_id = "none";
    const column_id = board.columns[col_idx].id;
    dialog_title = m.task_add_card;
    dialog_button_text = m.common_add;
    dialog_callback = () =>
      board.add_new_task(column_id, dialog_task, null, onCreated);
    reset_task(dialog_task);
    dialog_task.due_time = due_time ? new Date(due_time) : undefined;
    task_dialog_open = true;
  }

  function open_add_task_shortcut() {
    open_add_task();
  }

  function open_add_task(onCreated?: (id: string) => void) {
    if (active_board_read_only) {
      return;
    }
    pending_import_task = null;
    pending_task_due_time = undefined;
    pending_task_created = onCreated;
    if (board.columns.length === 0) {
      open_add_column_dialog();
      return;
    }
    task_column_id = "";
    task_column_error = false;
    task_column_dialog_open = true;
  }

  function open_add_task_for_date(date: Date) {
    pending_task_created = undefined;
    if (active_board_read_only) {
      return;
    }
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
    const column_idx = board.columns.findIndex(
      (column) => column.id === task_column_id,
    );
    if (column_idx === -1) {
      task_column_error = true;
      return;
    }
    task_column_dialog_open = false;
    if (pending_import_task) {
      open_imported_task(column_idx, pending_import_task);
      pending_import_task = null;
    } else {
      add_task_routine(column_idx, pending_task_due_time, pending_task_created);
    }
    pending_task_due_time = undefined;
    pending_task_created = undefined;
  }

  function open_imported_task(column_idx: number, task: Task) {
    editing_template_id = null;
    creating_task = true;
    dialog_template_id = "none";
    const column_id = board.columns[column_idx].id;
    dialog_title = m.task_import;
    dialog_button_text = m.common_import;
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
    const template = board.templates.find(
      (candidate) => candidate.id === template_id,
    );
    if (!template) {
      return;
    }
    editing_template_id = template.id;
    creating_task = false;
    dialog_template_id = "none";
    editing_template_name = template.name;
    dialog_title = m.task_template_edit;
    dialog_button_text = m.task_template_update;
    dialog_task = clone_task(template.task);
    dialog_callback = () =>
      board.update_task_template(
        template.id,
        editing_template_name,
        dialog_task,
      );
    templates_dialog_open = false;
    task_dialog_open = true;
  }

  function apply_template_to_new_task(template_id: string) {
    if (!creating_task) {
      return;
    }

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
    if (!name || saving_template) {
      return;
    }
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
    const column = board.columns.find(
      (candidate) => candidate.id === template_column_id,
    );
    if (!template || !column) {
      template_selection_error = true;
      return;
    }

    editing_template_id = null;
    creating_task = true;
    dialog_template_id = template.id.toString();
    dialog_title = m.task_template_create_card;
    dialog_button_text = m.common_add;
    dialog_task = task_from_template(template.task);
    dialog_callback = () => board.add_new_task(column.id, dialog_task);
    templates_dialog_open = false;
    task_dialog_open = true;
  }

  async function prepare_import() {
    const candidate = await board.prepare_import_from_file();
    if (!candidate) {
      return;
    }
    import_candidate = candidate;
    import_confirm_open = true;
  }

  async function confirm_import() {
    if (!import_candidate || importing) {
      return;
    }
    importing = true;
    const importing_indicator_timeout = window.setTimeout(() => {
      show_importing = true;
    }, 300);
    try {
      if (await board.import_board_as_new(import_candidate)) {
        reset_board_scoped_ui();
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
      const task_idx = board.columns[column_idx].tasks.findIndex(
        (task) => task.id === task_id,
      );
      if (task_idx !== -1) {
        return { column_idx, task_idx };
      }
    }
    return undefined;
  }

  function edit_task_by_id(task: Task) {
    const position = find_task_position(task.id);
    if (position) {
      edit_task_routine(position.column_idx, position.task_idx);
    }
  }

  function view_task_by_id(task: Task, read_only = active_board_read_only) {
    view_task_routine(
      task,
      read_only || active_board_read_only
        ? undefined
        : () => edit_task_by_id(task),
    );
  }

  function view_archived_task(task: Task) {
    view_task_routine(task);
  }

  async function prepare_task_import(column_id?: string) {
    if (board.columns.length === 0) {
      toast.error(m.ui_create_a_column_before_importing_a_task());
      return;
    }
    if (column_id && !board.columns.some((column) => column.id === column_id)) {
      toast.error(m.column_missing());
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
      logger.warn("task.share_text_import.failed", error);
      task_import_error =
        error instanceof Error
          ? error.message
          : m.ui_invalid_task_sharing_text();
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
    pending_task_created = undefined;
    task_column_id = "";
    task_column_error = false;
    task_column_dialog_open = true;
  }

  async function share_task(task: Task) {
    try {
      task_share_text = await serialize_portable_task(task);
      task_share_dialog_open = true;
    } catch (error) {
      logger.warn("task.share_text_prepare.failed", error);
      toast.error(m.ui_couldn_t_prepare_task_sharing_text());
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
        if (!copied) {
          throw new Error("Copy command failed");
        }
      }
      toast.success(m.ui_task_sharing_text_copied());
    } catch (error) {
      logger.warn("task.share_text_copy.failed", error);
      toast.error(m.ui_couldn_t_copy_text_select_it_and_copy_manually());
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

    toast.error(m.ui_referenced_card_no_longer_exists());
  }

  function stop_task_recurrence(task: Task) {
    const updated = clone_task(task);
    updated.recurrence = undefined;
    void board.update_task(task.id, updated);
  }

  function reschedule_task(task: Task, dates: TaskDates) {
    const updated = clone_task(task);
    updated.start_time = dates.start_time && new Date(dates.start_time);
    updated.due_time = dates.due_time && new Date(dates.due_time);
    updated.start_at_midnight = dates.start_at_midnight;
    updated.due_at_midnight = dates.due_at_midnight;
    void board.update_task(task.id, updated);
  }

  function switch_view(view: WorkspaceView) {
    selected_view = view;
    if (view === "calendar" || view === "gantt") {
      void board.ensure_archives_loaded();
    }
    const sequence = ++view_switch_sequence;

    // Paint the control feedback first, then render the heavier view content.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (sequence === view_switch_sequence) {
          active_view = view;
        }
      });
    });
  }
</script>

<div class="flex h-screen min-h-0 flex-col bg-background">
  {#if startup_error}
    <StartupErrorScreen error={startup_error} />
  {:else if !startup_check_finished}
    <div class="flex min-h-0 flex-1" aria-hidden="true"></div>
  {:else if board.column_fetch_error}
    <Empty.Root class="w-full">
      <Empty.Header>
        <Empty.Media variant="icon">
          <BugIcon />
        </Empty.Media>
        <Empty.Title>{m.common_error()}</Empty.Title>
        <Empty.Description>{m.board_load_error()}</Empty.Description>
      </Empty.Header>
      <Empty.Content>
        <Button onclick={() => board.get_columns()}>{m.common_retry()}</Button>
      </Empty.Content>
    </Empty.Root>
  {:else if !board.column_fetch_finish}
    <Empty.Root class="w-full">
      <Empty.Header>
        <Empty.Media variant="icon">
          <Spinner />
        </Empty.Media>
        <Empty.Title>{m.board_loading()}</Empty.Title>
        <Empty.Description>{m.board_loading_description()}</Empty.Description>
      </Empty.Header>
    </Empty.Root>
  {:else}
    <BoardSidebar
      bind:open={board_panel_open}
      boards={board.boards}
      active_board_id={board.active_board_id}
      onSwitch={(id) => void switch_board(id)}
      onCreate={create_board}
      onRename={(id, name) => board.rename_board(id, name)}
      onLoadArchivedCount={(id) => board.get_archived_count(id)}
      onDelete={delete_board}
    />
    <!-- `min-h-0` is essential here: without it this flex item grows to its
         contents and the inner viewport never becomes a vertical scroll
         container. -->
    <main class="flex min-h-0 min-w-0 flex-1 flex-col">
      <AppToolbar
        bind:board_panel_open
        bind:search_text
        search_pending={board.search_pending}
        search_error={board.search_error}
        bind:note_open
        {selected_view}
        update_check_in_progress={updates.in_progress}
        read_only={active_board_read_only}
        web_publish_count={share_sync.summary.active_count}
        {pending_device_count}
        {removed_access_count}
        onOpenDeviceRequests={() => {
          iroh_show_requests = true;
          iroh_show_received = false;
          iroh_share_dialog_open = true;
        }}
        onOpenRemovedAccess={() => {
          iroh_show_requests = false;
          iroh_show_received = true;
          iroh_share_dialog_open = true;
        }}
        web_publish_status={share_sync.summary.failed
          ? "error"
          : share_sync.summary.updating
            ? "updating"
            : "idle"}
        web_publish_detail={share_sync.summary.failed
          ? m.share_sync_failed({ error: share_sync.summary.failed.error })
          : share_sync.summary.updating
            ? m.ui_updating_published_views()
            : m.share_sync_summary({
                count: share_sync.summary.active_count,
                time: share_time_formatter.format(
                  new Date(share_sync.summary.latest_update),
                ),
              })}
        onPrepareImport={prepare_import}
        onExportAllBoards={() => {
          void board.export_all_boards_to_file();
        }}
        onImportAllBoards={() => {
          void restore_everything();
        }}
        onPrepareTaskImport={prepare_task_import}
        onOpenBoardShare={() => {
          board_share_dialog_open = true;
        }}
        onOpenIrohShare={() => {
          iroh_show_requests = false;
          iroh_show_received = false;
          iroh_share_dialog_open = true;
        }}
        onOpenTaskTemplates={open_templates_dialog}
        onOpenAllTasks={() => (all_task_open = true)}
        onAddTask={open_add_task_shortcut}
        onAddColumn={open_add_column_dialog}
        onCheckForUpdates={(manual) => updates.check(manual)}
      />

      <WorkspaceTabs
        {selected_view}
        active_board_name={board.boards.find(
          (item) => item.id === board.active_board_id,
        )?.name ?? m.board_title()}
        shared_role={active_board_summary?.shared_role}
        sync_status={active_board_summary?.sync_status}
        last_synced_at={active_board_summary
          ? board.iroh_last_synced_at[active_board_summary.id]
          : undefined}
        onSync={active_board_summary &&
        (active_board_summary.shared_role === "editor" ||
          (active_board_summary.shared_role === "owner" &&
            active_board_summary.is_shared))
          ? () => void board.sync_iroh_board(active_board_summary.id)
          : undefined}
        onSwitchView={switch_view}
      />

      <AllTaskExplorer
        bind:open={all_task_open}
        active_board_id={board.active_board_id}
        boards={board.boards}
        columns={board.columns}
        all_task_items={board.all_task_items}
        all_task_items_loading={board.all_task_items_loading}
        all_task_items_error={board.all_task_items_error}
        all_task_items_has_more={board.all_task_items_has_more}
        read_only={active_board_read_only}
        onSwitchBoard={switch_board}
        onLoadMoreAllTasks={board.load_more_all_tasks.bind(board)}
        onSearchAllTasks={board.search_all_tasks.bind(board)}
        onGetArchiveDetail={board.get_archive_detail.bind(board)}
        onGetExpiredDetail={board.get_expired_task_detail.bind(board)}
        onUnarchive={board.unarchive_task_from_list.bind(board)}
        onViewTask={view_task_by_id}
        onViewArchivedTask={view_archived_task}
        onEditTask={edit_task_by_id}
        onArchiveTask={(task) => void board.archive_task(task.id)}
      />

      <AddColumnDialog
        bind:open={column_dialog_open}
        bind:name={dialog_column_name}
        bind:title_error={column_dialog_title_error}
        onSubmit={handle_add_column}
      />

      <NotePanel
        {requested_note}
        onNotesChanged={() => note_revision++}
        bind:open={note_open}
        card_reference_options={board.columns.flatMap((column) =>
          column.tasks.map((task) => ({ task, column_name: column.name })),
        )}
      />

      <BoardShareDialog
        bind:open={board_share_dialog_open}
        columns={board.columns}
        default_title={updates.app_name}
        active_board_id={board.active_board_id}
        boards={board.boards}
        onRetireShare={(share_id) => share_sync.retire(share_id)}
        onRetryShareStorage={async () => {
          restore_managed_share_state(
            board.boards.length === 1 ? board.boards[0].id : undefined,
          );
          if (!managed_share_state.storage_error) {
            await share_sync.restore_all();
          }
        }}
        onShareRevokeError={(share_id, error) => {
          if (
            !managed_share_state.shares.some((share) => share.id === share_id)
          ) {
            return;
          }
          managed_share_state.set_sync_state(
            share_id,
            "error",
            m.share_revoke_retry(),
          );
        }}
      />
      <IrohShareDialog
        bind:open={iroh_share_dialog_open}
        show_requests={iroh_show_requests}
        show_received={iroh_show_received}
        onRequestsChanged={() => void refresh_device_requests()}
        {board}
      />

      <TaskColumnDialog
        bind:open={task_column_dialog_open}
        bind:selected_column_id={task_column_id}
        bind:selection_error={task_column_error}
        column_items={task_column_items}
        onSubmit={handle_task_column_select}
        title={pending_import_task ? m.task_import() : m.task_add_card()}
        description={pending_import_task
          ? m.ui_select_the_column_where_this_task_should_be_imported()
          : m.ui_select_the_column_for_the_new_card()}
        submit_label={pending_import_task ? m.common_import() : m.ui_continue()}
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
        onDeleteTemplate={(template_id) =>
          board.delete_task_template(template_id)}
        onSaveTemplate={save_task_template}
      />

      <EditTaskDialog
        bind:open={task_dialog_open}
        bind:task={dialog_task}
        label_suggestions={board.labels}
        card_reference_options={board.columns.flatMap((column) =>
          column.tasks.map((task) => ({ task, column_name: column.name })),
        )}
        dialog_title={dialog_title()}
        submit_button_text={dialog_button_text()}
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
        onEdit={active_board_read_only ? undefined : view_task_edit_callback}
        history={board.active_board_id === null
          ? undefined
          : { board_id: board.active_board_id, columns: board.columns }}
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

      {#if active_view === "flow" && board.active_board_id !== null}
        <div class="min-h-0 min-w-0 flex-1">
          {#key board.active_board_id}
            <FlowView
              {note_revision}
              onOpenNote={(id) => {
                requested_note = { id };
                note_open = true;
              }}
              board_id={board.active_board_id}
              columns={board.columns}
              archives={board.archives}
              onViewArchivedTask={view_archived_task}
              onArchiveTask={(task) => board.archive_task(task.id)}
              onDeleteTask={(task) => void board.delete_task(task.id)}
              search_task_ids={board.search_task_ids}
              onViewTask={view_task_by_id}
              onEditTask={edit_task_by_id}
              read_only={active_board_read_only}
            />
          {/key}
        </div>
      {:else if active_view === "gantt" && !(search_text.trim() && (board.search_pending || board.search_error))}
        <!-- The Gantt chart scrolls itself to keep its header and task list in view. -->
        <div class="min-h-0 min-w-0 flex-1">
          <GanttView
            columns={board.columns}
            archives={board.archives}
            {search_text}
            search_task_ids={board.search_task_ids}
            bind:scale={gantt_scale}
            bind:show_archived={gantt_show_archived}
            onViewTask={view_task_by_id}
            onViewArchivedTask={view_archived_task}
            onEditTask={edit_task_by_id}
            onDuplicateTask={(task) => board.duplicate_task(task.id)}
            onSaveAsTemplate={open_save_template_dialog}
            onExportTask={share_task}
            onArchiveTask={(task) => board.archive_task(task.id)}
            onUnarchiveTask={(column_id, task_id) =>
              board.unarchive_task(column_id, task_id)}
            onDeleteTask={(task) => board.delete_task(task.id)}
            onRescheduleTask={reschedule_task}
            read_only={active_board_read_only}
          />
        </div>
      {:else}
        <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="both">
          <div
            class={active_view === "board"
              ? "min-h-full px-5 pb-5 pt-7"
              : "min-h-full p-5"}
          >
            {#if active_view === "statistics"}
              <StatisticsView
                board_id={board.active_board_id}
                archives={board.archives}
                columns={board.columns}
                loaded={board.archives_loaded}
                loading={board.archives_loading}
                onRetry={() => void board.get_archives()}
                onViewTask={(task) => view_task_by_id(task)}
              />
            {:else if search_text.trim() && board.search_pending}
              <div
                class="flex min-h-[50vh] items-center justify-center text-sm text-muted-foreground"
                role="status"
              >
                {m.board_searching_cards()}
              </div>
            {:else if search_text.trim() && board.search_error}
              <div
                class="flex min-h-[50vh] flex-col items-center justify-center gap-2 text-sm text-destructive"
                role="alert"
              >
                <span>{m.board_search_unavailable()}</span>
                <Button
                  variant="link"
                  size="sm"
                  onclick={() => board.search_tasks(search_text)}
                >
                  {m.board_search_retry()}
                </Button>
              </div>
            {:else if active_view === "today"}
              <TodayView
                columns={board.columns}
                {search_text}
                search_task_ids={board.search_task_ids}
                onViewTask={view_task_by_id}
                onEditTask={edit_task_by_id}
                onDuplicateTask={(task) => board.duplicate_task(task.id)}
                onSaveAsTemplate={open_save_template_dialog}
                onExportTask={share_task}
                onAddTask={open_add_task_for_date}
                onArchiveTask={(task) => board.archive_task(task.id)}
                onDeleteTask={(task) => board.delete_task(task.id)}
                read_only={active_board_read_only}
              />
            {:else if active_view === "calendar"}
              <CalendarView
                columns={board.columns}
                archives={board.archives}
                {search_text}
                search_task_ids={board.search_task_ids}
                bind:visible_date={calendar_visible_date}
                bind:view_mode={calendar_view_mode}
                bind:show_archived={calendar_show_archived}
                bind:show_recurring_previews={calendar_show_recurring_previews}
                expand_all={board.calendar_expand_all}
                onExpandAllChange={(expanded) =>
                  void board.set_calendar_expand_all(expanded)}
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
                read_only={active_board_read_only}
              />
            {:else}
              <BoardView
                {search_text}
                search_task_ids={board.search_task_ids}
                task_expand_mode={board.task_expand_mode}
                onAddColumn={open_add_column_dialog}
                onAddTask={add_task_routine}
                onImportTask={(column_id) =>
                  void prepare_task_import(column_id)}
                onViewTask={(task, column_idx, task_idx) =>
                  view_task_routine(task, () =>
                    edit_task_routine(column_idx, task_idx),
                  )}
                onEditTask={edit_task_routine}
                onSaveAsTemplate={open_save_template_dialog}
                onExportTask={share_task}
                read_only={active_board_read_only}
              />
            {/if}
          </div>
        </ScrollArea>
      {/if}

      {#if card_reference_preview}
        <CardReferencePreview
          task={card_reference_preview_content?.task}
          location={card_reference_preview_content?.location}
          left={card_reference_preview.left}
          top={card_reference_preview.top}
          show_below={card_reference_preview.show_below}
        />
      {/if}
    </main>
  {/if}
</div>

{#each board.iroh_removal_notices.slice(0, 1) as notice (notice)}
  <IrohRemovalNotice
    {notice}
    onacknowledge={() => {
      board.iroh_removal_notices = board.iroh_removal_notices.slice(1);
    }}
  />
{/each}
