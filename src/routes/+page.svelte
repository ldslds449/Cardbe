<script lang="ts">
import { logger } from "$lib/logger";
import { onMount, untrack } from "svelte";
import { getName, getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ModeWatcher } from "mode-watcher";
import { toast } from "svelte-sonner";
import { Button } from "$lib/components/ui/button/index.js";
import { Spinner } from "$lib/components/ui/spinner/index.js";
import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
import * as Empty from "$lib/components/ui/empty/index.js";
import * as Card from "$lib/components/ui/card/index.js";

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
import IrohShareDialog from "./components/dialog/iroh_share_dialog.svelte";
import CalendarView from "./components/calendar/calendar_view.svelte";
import type { CalendarViewMode } from "./components/calendar/calendar";
import FocusView from "./components/focus/focus_view.svelte";
import RecurringPanel from "./components/recurring/recurring_panel.svelte";
import NotePanel from "./components/note/note_panel.svelte";
import CardReferencePreview from "./components/card_reference_preview.svelte";
import AppToolbar from "./components/workspace/app_toolbar.svelte";
import BoardSidebar from "./components/workspace/board_sidebar.svelte";
import WorkspaceTabs from "./components/workspace/workspace_tabs.svelte";
import type { WorkspaceView } from "./components/workspace/workspace";
import {
  markUpdateCheckSuccessful,
  shouldCheckForUpdate,
  UPDATE_STARTUP_DELAY_MS,
  updateCheckErrorMessage,
} from "./utils/update";
import {
  parse_portable_task,
  serialize_portable_task,
} from "./utils/task-transfer";
import {
  build_share_snapshot,
  group_enabled_shares_by_board,
  is_managed_share_enabled,
  publish_share,
  revoke_managed_shares,
  revoke_share,
  resolve_share_selection,
  save_managed_share,
  share_content_signature,
} from "./share";
import {
  managed_share_state,
  restore_managed_share_state,
} from "./share-state.svelte";
import { LatestShareSyncQueue } from "./share-sync-queue";

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
let startup_error = $state<string | null>(null);
let app_name = $state("");
let app_name_promise: Promise<string> | undefined;
let update_check_in_progress = $state(false);
let startup_screen_dismissed = false;
let board_panel_open = $state(false);
const active_board_summary = $derived(
  board.boards.find((item) => item.id === board.active_board_id),
);
const active_board_read_only = $derived(
  active_board_summary?.shared_role === "viewer",
);

$effect(() => {
  if (startup_screen_dismissed || !board.column_fetch_finish) return;
  startup_screen_dismissed = true;
  // Let the workspace paint before the HTML-level startup card fades away.
  requestAnimationFrame(() =>
    window.dispatchEvent(new Event("cardbe:workspace-ready")),
  );
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
const share_time_formatter = new Intl.DateTimeFormat("en-US", {
  hour: "numeric",
  minute: "2-digit",
  second: "2-digit",
});
const share_sync_summary = $derived.by(() => {
  const active_shares = managed_share_state.shares.filter(
    is_managed_share_enabled,
  );
  const states = active_shares.map((share) =>
    managed_share_state.sync_state(share.id),
  );
  const failed = states.find((state) => state.status === "error");
  return {
    active_count: active_shares.length,
    failed,
    updating: states.some(
      (state) => state.status === "pending" || state.status === "syncing",
    ),
    latest_update: active_shares.reduce(
      (latest, share) => Math.max(latest, Date.parse(share.updated_at) || 0),
      0,
    ),
  };
});

// Publishing is asynchronous and the reactive content effect can run again
// while a previous publish is still in flight. Keep that work per-link so a
// second timer cannot leave the visible state at "Updating" forever.
type ShareSyncRequest = {
  share: (typeof managed_share_state.shares)[number];
  signature: string;
  columns: typeof board.columns;
};
const SHARE_SYNC_TIMEOUT_MS = 15_000;

function with_share_sync_timeout<T>(operation: Promise<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(
      () => reject(new Error("Sharing took too long. Please try again.")),
      SHARE_SYNC_TIMEOUT_MS,
    );
    void operation.then(
      (result) => {
        window.clearTimeout(timeout);
        resolve(result);
      },
      (error) => {
        window.clearTimeout(timeout);
        reject(error);
      },
    );
  });
}

$effect(() => {
  const shares = managed_share_state.shares.filter(
    (share) =>
      is_managed_share_enabled(share) &&
      share.board_id === board.active_board_id,
  );
  if (shares.length === 0 || !board.column_fetch_finish) return;

  const pending: Array<{ share: (typeof shares)[number]; signature: string }> =
    [];
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
        pending.push({ share, signature });
      }
    } catch (error) {
      logger.error("share.selection_refresh.failed", error);
      untrack(() =>
        managed_share_state.set_sync_state(
          share.id,
          "error",
          error instanceof Error
            ? error.message
            : "Shared content is no longer available",
        ),
      );
      void disable_stale_share(share);
    }
  }
  if (pending.length === 0) return;

  const timeout = window.setTimeout(() => {
    // Do not mark the link pending before this callback starts. Svelte may
    // cancel this debounce when the user switches boards; marking it earlier
    // was the source of links stranded at "Updating".
    for (const item of pending)
      void sync_managed_share(item.share, item.signature).catch(
        () => undefined,
      );
  }, 700);
  return () => window.clearTimeout(timeout);
});

const share_sync_queue = new LatestShareSyncQueue<ShareSyncRequest>(
  async (share_id, next, is_current) => {
    if (
      !managed_share_state.shares.some((candidate) => candidate.id === share_id)
    )
      return;
    if (managed_share_state.published_signatures[share_id] === next.signature) {
      managed_share_state.set_sync_state(share_id, "synced");
      return;
    }
    const selection = resolve_share_selection(
      next.columns,
      next.share.selected_column_ids,
      next.share.selected_task_ids,
      next.share.selected_labels,
    );
    const snapshot = build_share_snapshot(
      next.columns,
      selection.selected_column_ids,
      selection.selected_task_ids,
      next.share.title,
    );
    const refreshed = await with_share_sync_timeout(
      publish_share(
        snapshot,
        next.share.selected_column_ids,
        next.share.selected_task_ids,
        next.share.expires_at ? new Date(next.share.expires_at) : null,
        next.share,
        next.share.selected_labels,
      ),
    );
    refreshed.board_id = next.share.board_id;
    if (
      is_current() &&
      managed_share_state.shares.some((candidate) => candidate.id === share_id)
    ) {
      save_managed_share(refreshed, next.signature);
    }
  },
  (share_id) => {
    if (
      managed_share_state.shares.some((candidate) => candidate.id === share_id)
    ) {
      managed_share_state.set_sync_state(share_id, "syncing");
    }
  },
  (share_id, error) => {
    if (
      !managed_share_state.shares.some((candidate) => candidate.id === share_id)
    )
      return;
    console.error("Couldn't update a board share", error);
    logger.error("share.sync.failed", error);
    managed_share_state.set_sync_state(
      share_id,
      "error",
      error instanceof Error
        ? error.message
        : "Couldn't update the shared board",
    );
  },
);

function sync_managed_share(
  share: (typeof managed_share_state.shares)[number],
  signature: string,
  columns = board.columns,
): Promise<void> {
  if (
    !managed_share_state.shares.some((candidate) => candidate.id === share.id)
  )
    return Promise.resolve();
  // The returned promise resolves only after this snapshot (or a later
  // snapshot that superseded it) reaches a terminal state. Board switching
  // therefore cannot outrun a publish already in flight.
  return share_sync_queue.request(share.id, { share, signature, columns });
}

async function sync_shares_for_board(
  board_id: number,
  columns: typeof board.columns,
) {
  const shares = managed_share_state.shares.filter(
    (share) => is_managed_share_enabled(share) && share.board_id === board_id,
  );
  await Promise.all(
    shares.map(async (share) => {
      const selection = resolve_share_selection(
        columns,
        share.selected_column_ids,
        share.selected_task_ids,
        share.selected_labels,
      );
      const signature = share_content_signature(
        columns,
        selection.selected_column_ids,
        selection.selected_task_ids,
        share.title,
      );
      if (signature !== managed_share_state.published_signatures[share.id]) {
        await sync_managed_share(share, signature, columns);
      }
    }),
  );
}

async function restore_all_managed_shares() {
  const grouped = group_enabled_shares_by_board(managed_share_state.shares);
  await Promise.all(
    [...grouped.entries()].map(async ([board_id, shares]) => {
      try {
        const serialized = await invoke<ColumnSerialized[]>(
          "get_board_columns",
          { boardId: board_id },
        );
        await sync_shares_for_board(
          board_id,
          serialized.map(deserialize_column),
        );
      } catch (error) {
        logger.error("share.restore.failed", error);
        console.error(`Couldn't restore shares for board ${board_id}`, error);
        for (const share of shares) await disable_stale_share(share);
        toast.error(
          "Sharing was stopped for a link whose board no longer exists.",
        );
      }
    }),
  );
}

async function disable_stale_share(
  share: (typeof managed_share_state.shares)[number],
) {
  try {
    share_sync_queue.retire(share.id);
    await revoke_share(share);
    if (
      managed_share_state.shares.some((candidate) => candidate.id === share.id)
    ) {
      managed_share_state.forget(share.id);
      toast.error(
        `Sharing stopped for “${share.title}” because its content could not be updated.`,
      );
    }
  } catch (error) {
    logger.error("share.stale_revoke.failed", error);
    if (
      managed_share_state.shares.some((candidate) => candidate.id === share.id)
    ) {
      managed_share_state.set_sync_state(
        share.id,
        "error",
        "Could not disable outdated share content. Close Cardbe to stop sharing.",
      );
    }
  }
}

async function flush_active_board_shares() {
  const shares = managed_share_state.shares.filter(
    (share) =>
      is_managed_share_enabled(share) &&
      share.board_id === board.active_board_id,
  );
  await Promise.all(
    shares.map(async (share) => {
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
      if (signature !== managed_share_state.published_signatures[share.id])
        await sync_managed_share(share, signature);
    }),
  );
  const failed = shares
    .map((share) => managed_share_state.sync_state(share.id))
    .find((state) => state.status === "error");
  if (failed)
    throw new Error(
      `Couldn't update a share before switching boards: ${failed.error}`,
    );
}

function reset_board_scoped_ui() {
  search_text = "";
  archive_open = false;
  expired_open = false;
  recurring_open = false;
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
  editing_template_id = null;
  task_import_target_column_id = null;
}

async function switch_board(id: number) {
  if (id === board.active_board_id) {
    board_panel_open = false;
    return;
  }
  try {
    await flush_active_board_shares();
    if (await board.switch_board(id)) reset_board_scoped_ui();
  } catch (error) {
    logger.error("board.switch_share_sync.failed", error);
    toast.error(
      error instanceof Error ? error.message : "Couldn't switch board",
    );
  }
}

async function delete_board(id: number): Promise<boolean> {
  try {
    for (const share of managed_share_state.shares) {
      if (share.board_id === id) share_sync_queue.retire(share.id);
    }
    await revoke_managed_shares((share) => share.board_id === id);
    const deleted = await board.delete_board(id);
    if (deleted) reset_board_scoped_ui();
    return deleted;
  } catch (error) {
    logger.error("board.delete_share_revoke.failed", error);
    // `revoke_managed_shares` forgets each link only after its own revoke
    // succeeds. The remaining links are still live, but their queues were
    // retired above, so make their terminal failure visible and retryable.
    for (const share of managed_share_state.shares) {
      if (share.board_id === id) {
        managed_share_state.set_sync_state(
          share.id,
          "error",
          "Couldn't revoke this share link. Try again before deleting this board.",
        );
      }
    }
    toast.error(
      error instanceof Error
        ? error.message
        : "Couldn't revoke this board's share links",
    );
    return false;
  }
}

async function create_board(name: string): Promise<boolean> {
  const created = await board.create_board(name);
  if (created) reset_board_scoped_ui();
  return created;
}

async function restore_everything() {
  try {
    if (
      await board.import_all_boards_from_file(true, async () => {
        for (const share of managed_share_state.shares)
          share_sync_queue.retire(share.id);
        await revoke_managed_shares(() => true);
      })
    )
      reset_board_scoped_ui();
  } catch (error) {
    logger.error("backup.restore_share_revoke.failed", error);
    // Successfully revoked links have already been forgotten. Only links
    // still present failed to revoke and must not be left as Updating.
    for (const share of managed_share_state.shares) {
      managed_share_state.set_sync_state(
        share.id,
        "error",
        "Couldn't revoke this share link. Try again before restoring everything.",
      );
    }
    toast.error(
      error instanceof Error
        ? error.message
        : "Restore stopped because share links could not be revoked",
    );
  }
}

onMount(() => {
  let device_request_timer: number | undefined;
  void invoke<string | null>("get_startup_error")
    .then((error) => {
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
        restore_managed_share_state(
          board.boards.length === 1 ? board.boards[0].id : undefined,
        );
        await restore_all_managed_shares();
      });
    })
    .catch((error) => {
      startup_error = `Could not check local data startup status: ${String(error)}`;
      requestAnimationFrame(() =>
        window.dispatchEvent(new Event("cardbe:workspace-ready")),
      );
    });
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
    )
      return;

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
  window.addEventListener("cardbe:show-card-preview", handle_show_card_preview);
  window.addEventListener("cardbe:hide-card-preview", handle_hide_card_preview);
  const data_changed_listener = listen<{ kind?: string; boardId?: number }>(
    "cardbe:data-changed",
    (event) => {
      if (
        event.payload.kind !== "task" ||
        !Number.isInteger(event.payload.boardId)
      )
        return;
      const changed_board_id = event.payload.boardId!;
      if (changed_board_id === board.active_board_id) {
        board.get_columns();
        return;
      }
      void invoke<ColumnSerialized[]>("get_board_columns", {
        boardId: changed_board_id,
      })
        .then((columns) =>
          sync_shares_for_board(
            changed_board_id,
            columns.map(deserialize_column),
          ),
        )
        .catch((error) => {
          console.error("Couldn't sync Quick Add board shares", error);
          logger.error("share.quick_add_sync.failed", error);
        });
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
    if (device_request_timer !== undefined)
      window.clearInterval(device_request_timer);
    window.clearTimeout(update_check_timeout);
    void data_changed_listener.then((unlisten) => unlisten());
    void iroh_remote_push_listener.then((unlisten) => unlisten());
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
    const update = await invoke<{ version: string; url: string } | null>(
      "check_for_update",
      {
        currentVersion: current_version,
      },
    );
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
              logger.warn("update.open_release.failed", error);
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
    logger.warn("update.manual_check.failed", error);
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
  if (active_board_read_only) return;
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
  if (active_board_read_only) return;
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
  if (active_board_read_only) return;
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
  if (active_board_read_only) return;
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
  if (active_board_read_only) return;
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
  const template = board.templates.find(
    (candidate) => candidate.id === template_id,
  );
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
    logger.warn("task.share_text_import.failed", error);
    task_import_error =
      error instanceof Error ? error.message : "Invalid task sharing text";
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
    logger.warn("task.share_text_prepare.failed", error);
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
    logger.warn("task.share_text_copy.failed", error);
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

  {#if startup_error}
    <main
      class="grid min-h-0 flex-1 place-items-center overflow-y-auto bg-muted/30 p-6"
      aria-labelledby="startup-error-title"
    >
      <Card.Root
        class="w-full max-w-2xl gap-0 overflow-hidden border-border bg-card p-0 shadow-sm"
      >
        <header class="flex items-start gap-4 border-b px-6 py-6">
          <div
            class="flex size-11 shrink-0 items-center justify-center rounded-xl bg-destructive/10 text-destructive ring-1 ring-inset ring-destructive/20"
            aria-hidden="true"
          >
            <BugIcon class="size-5" />
          </div>
          <div class="min-w-0 space-y-1.5">
            <p
              class="text-xs font-medium uppercase tracking-wider text-muted-foreground"
            >
              Local database
            </p>
            <h1
              id="startup-error-title"
              class="text-lg font-semibold tracking-tight"
            >
              Couldn't open your data
            </h1>
            <p class="text-sm leading-relaxed text-muted-foreground">
              Cardbe opened, but couldn't read its local database. The workspace
              can't load until this issue is resolved.
            </p>
          </div>
        </header>

        <Card.Content class="grid gap-3 py-5">
          <div class="flex items-center gap-2 text-sm font-medium">
            <span class="size-2 rounded-full bg-destructive" aria-hidden="true"></span>
            Error details
          </div>
          <pre
            class="max-h-[40vh] overflow-auto whitespace-pre-wrap break-all rounded-lg border border-destructive/20 bg-muted/50 px-4 py-3 font-mono text-xs leading-relaxed text-foreground selection:bg-primary/20"
            role="alert"
          >{startup_error}</pre>
          <p class="text-xs text-muted-foreground">
            This error is also recorded in the Cardbe app log.
          </p>
        </Card.Content>
      </Card.Root>
    </main>
  {:else if board.column_fetch_error}
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
    <BoardSidebar
      bind:open={board_panel_open}
      boards={board.boards}
      active_board_id={board.active_board_id}
      onSwitch={(id) => void switch_board(id)}
      onCreate={create_board}
      onRename={(id, name) => board.rename_board(id, name)}
      onDelete={delete_board}
    />
    <!-- `min-h-0` is essential here: without it this flex item grows to its
         contents and the inner viewport never becomes a vertical scroll
         container. -->
    <main class="flex min-h-0 min-w-0 flex-1 flex-col">
      <AppToolbar
        bind:board_panel_open
        bind:search_text
        bind:archive_open
        bind:expired_open
        bind:recurring_open
        bind:note_open
        bind:task_expand_mode
        {selected_view}
        {update_check_in_progress}
        read_only={active_board_read_only}
        web_publish_count={share_sync_summary.active_count}
        {pending_device_count}
        {removed_access_count}
        onOpenDeviceRequests={() => { iroh_show_requests = true; iroh_show_received = false; iroh_share_dialog_open = true; }}
        onOpenRemovedAccess={() => { iroh_show_requests = false; iroh_show_received = true; iroh_share_dialog_open = true; }}
        web_publish_status={share_sync_summary.failed ? "error" : share_sync_summary.updating ? "updating" : "idle"}
        web_publish_detail={share_sync_summary.failed
        ? `Web publish failed: ${share_sync_summary.failed.error}`
        : share_sync_summary.updating
          ? "Updating published views"
          : `${share_sync_summary.active_count} ${share_sync_summary.active_count === 1 ? "web view" : "web views"} published · Last published ${share_time_formatter.format(new Date(share_sync_summary.latest_update))}`}
        onPrepareImport={prepare_import}
        onExportAllBoards={() => { void board.export_all_boards_to_file(); }}
        onImportAllBoards={() => { void restore_everything(); }}
        onPrepareTaskImport={prepare_task_import}
        onOpenBoardShare={() => { board_share_dialog_open = true; }}
        onOpenIrohShare={() => { iroh_show_requests = false; iroh_show_received = false; iroh_share_dialog_open = true; }}
        onOpenTaskTemplates={open_templates_dialog}
        onAddTask={open_add_task_shortcut}
        onAddColumn={open_add_column_dialog}
        onCheckForUpdates={check_for_updates}
      />

      <WorkspaceTabs
        {selected_view}
        active_board_name={board.boards.find((item) => item.id === board.active_board_id)?.name ?? "Board"}
        shared_role={active_board_summary?.shared_role}
        sync_status={active_board_summary?.sync_status}
        last_synced_at={active_board_summary ? board.iroh_last_synced_at[active_board_summary.id] : undefined}
        onSync={active_board_summary && active_board_summary.shared_role !== "owner" ? () => void board.sync_iroh_board(active_board_summary.id) : undefined}
        onSwitchView={switch_view}
      />

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
        active_board_id={board.active_board_id}
        boards={board.boards}
        onRetireShare={(share_id) => share_sync_queue.retire(share_id)}
        onShareRevokeError={(share_id, error) => {
        if (!managed_share_state.shares.some((share) => share.id === share_id)) return;
        managed_share_state.set_sync_state(
          share_id,
          "error",
          error instanceof Error ? error.message : "Couldn't revoke this share link. Try again.",
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
        column.tasks.map((task) => ({ task, column_name: column.name })))}
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
        onEdit={active_board_read_only ? undefined : view_task_edit_callback}
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

      <ScrollArea class="min-h-0 min-w-0 flex-1" orientation="both">
        <div
          class={active_view === "board" ? "min-h-full px-5 pb-5 pt-7" : "min-h-full p-5"}
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
              read_only={active_board_read_only}
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
              read_only={active_board_read_only}
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
              read_only={active_board_read_only}
            />
          {/if}
        </div>
      </ScrollArea>

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
