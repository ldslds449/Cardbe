import {
  parseCommandError,
  translateCommandError,
  type CommandError,
} from "#lib/command-errors.js";
import { m } from "#lib/paraglide/messages.js";
import { logger } from "#lib/logger.js";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";
import { BoardStoreBase, type BoardSummary } from "./board-base.svelte";

const IROH_BACKGROUND_SYNC_MS = 30_000;
const IROH_VIEWER_SYNC_MS = 180_000;

interface IrohSyncResult {
  board: BoardSummary;
  content_changed: boolean;
  owner_sync?: { synced_devices: number; failed_devices: number } | null;
}

// Iroh board sharing: invitations, background synchronization, access
// recovery, conflicts, and removal of revoked shared boards.
export abstract class IrohBoardStore extends BoardStoreBase {
  iroh_enabled = $state(false);
  private iroh_sync_timer: number | undefined;
  private readonly handle_online = () => this.trigger_iroh_background_sync();
  private readonly handle_visibility_change = () => {
    if (document.visibilityState === "visible") {
      this.trigger_iroh_background_sync();
    }
  };
  private iroh_syncing = new Set<number>();
  private iroh_failures = new Map<number, number>();
  private iroh_retry_after = new Map<number, number>();
  private iroh_last_checked_at = new Map<number, number>();
  private iroh_host_checking = false;
  private iroh_network_generation = 0;
  iroh_last_error = $state<CommandError | null>(null);
  iroh_sync_error = $state<Record<number, CommandError>>({});
  iroh_access_removed = $state<Record<number, boolean>>({});
  iroh_access_error = $state<Record<number, CommandError>>({});
  iroh_last_synced_at = $state<Record<number, number>>({});

  async create_iroh_invite(
    board_id: number,
    permission: "viewer" | "editor",
  ): Promise<string> {
    const ticket = await invoke<string>("create_iroh_invite", {
      boardId: board_id,
      permission,
    });
    await this.get_boards();
    return ticket;
  }

  async join_iroh_invite(ticket: string, silent = false): Promise<boolean> {
    this.iroh_last_error = null;
    try {
      const joined = await invoke<BoardSummary>("join_iroh_invite", {
        ticket,
        requestApproval: !silent,
      });
      this.boards = [...this.boards, joined];
      this.iroh_last_synced_at = {
        ...this.iroh_last_synced_at,
        [joined.id]: Date.now(),
      };
      await this.switch_board(joined.id);
      return true;
    } catch (error) {
      logger.error("iroh.invite_join.failed", error);
      this.iroh_last_error = parseCommandError(error) ?? {
        code: "INTERNAL_ERROR",
      };
      if (!silent && this.iroh_last_error.code !== "SHARE_APPROVAL_REQUIRED") {
        toast.error(translateCommandError(this.iroh_last_error));
      }
      return false;
    }
  }

  async sync_iroh_board(
    board_id = this.active_board_id,
    silent = false,
    access_only = false,
  ): Promise<boolean> {
    if (board_id === null || !this.iroh_enabled) {
      return false;
    }
    if (this.iroh_syncing.has(board_id)) {
      return false;
    }
    const previous_status = this.boards.find(
      (item) => item.id === board_id,
    )?.sync_status;
    const network_generation = this.iroh_network_generation;
    this.iroh_syncing.add(board_id);
    if (!access_only) {
      this.update_iroh_summary(board_id, "syncing");
    }
    try {
      if (access_only) {
        const [approved, device_id, refreshed] = await invoke<
          [boolean, string, BoardSummary?]
        >("request_iroh_board_access", {
          boardId: board_id,
          requestApproval: false,
        });
        if (!approved) {
          throw { code: "SHARE_APPROVAL_REQUIRED", device_id };
        }
        if (refreshed) {
          this.apply_iroh_conflict_refresh(refreshed);
        }
        this.iroh_failures.delete(board_id);
        this.iroh_retry_after.delete(board_id);
        delete this.iroh_sync_error[board_id];
        delete this.iroh_access_error[board_id];
        this.iroh_access_removed[board_id] = false;
        return true;
      }
      const {
        board: synced,
        content_changed,
        owner_sync,
      } = await invoke<IrohSyncResult>("sync_iroh_board", {
        boardId: board_id,
      });
      this.iroh_failures.delete(board_id);
      this.iroh_retry_after.delete(board_id);
      delete this.iroh_sync_error[board_id];
      this.iroh_access_removed = {
        ...this.iroh_access_removed,
        [board_id]: false,
      };
      this.boards = this.boards.map((board) =>
        board.id === synced.id ? synced : board,
      );
      if (!owner_sync || owner_sync.synced_devices > 0) {
        this.iroh_last_synced_at = {
          ...this.iroh_last_synced_at,
          [synced.id]: Date.now(),
        };
      }
      if (board_id === this.active_board_id && content_changed) {
        this.can_undo = false;
        this.reload_active_board_data();
      }
      if (content_changed && this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      if (!silent) {
        if (owner_sync?.failed_devices) {
          toast.warning(
            m.share_owner_sync_partial({
              synced: owner_sync.synced_devices,
              failed: owner_sync.failed_devices,
            }),
          );
        } else if (owner_sync && owner_sync.synced_devices === 0) {
          toast.warning(m.share_owner_sync_no_editors());
        } else {
          toast.success(m.share_synced());
        }
      }
      return !owner_sync?.failed_devices;
    } catch (error) {
      if (network_generation !== this.iroh_network_generation) {
        if (!access_only && previous_status) {
          this.update_iroh_summary(board_id, previous_status);
        }
        return false;
      }
      logger.error("iroh.board_sync.failed", error);
      const command_error = parseCommandError(error) ?? {
        code: "INTERNAL_ERROR",
      };
      if (await this.remove_revoked_iroh_board(board_id, command_error)) {
        return false;
      }
      this.iroh_sync_error[board_id] = command_error;
      const access_error = [
        "SHARE_ACCESS_REVOKED",
        "INVITE_DISABLED",
        "INVITE_NOT_FOUND",
        "SHARE_APPROVAL_REQUIRED",
      ].includes(command_error.code);
      if (access_error) {
        this.iroh_access_error[board_id] = command_error;
        this.iroh_access_removed = {
          ...this.iroh_access_removed,
          [board_id]: true,
        };
      }
      const failures = Math.min(5, (this.iroh_failures.get(board_id) ?? 0) + 1);
      this.iroh_failures.set(board_id, failures);
      this.iroh_retry_after.set(
        board_id,
        Date.now() + Math.min(300_000, IROH_BACKGROUND_SYNC_MS * 2 ** failures),
      );
      await this.get_boards();
      if (
        this.boards.find((board) => board.id === board_id)?.sync_status !==
        "conflict"
      ) {
        this.update_iroh_summary(
          board_id,
          previous_status === "pending" && !access_error ? "pending" : "error",
        );
      }
      if (!silent) {
        toast.error(translateCommandError(error));
      }
      return false;
    } finally {
      // Access-only checks must also yield their place in the sync queue.
      this.iroh_last_checked_at.set(board_id, Date.now());
      this.iroh_syncing.delete(board_id);
      if (network_generation !== this.iroh_network_generation) {
        this.trigger_iroh_background_sync(true);
      }
    }
  }

  async restore_iroh_access(board_id: number) {
    this.iroh_access_removed = {
      ...this.iroh_access_removed,
      [board_id]: false,
    };
    delete this.iroh_access_error[board_id];
    delete this.iroh_sync_error[board_id];
    this.iroh_failures.delete(board_id);
    this.iroh_retry_after.delete(board_id);
    if (
      this.boards.find((item) => item.id === board_id)?.sync_status ===
      "conflict"
    ) {
      toast.success(m.ui_access_restored_conflict());
    } else if (await this.sync_iroh_board(board_id, true)) {
      toast.success(m.ui_access_restored());
    }
  }

  apply_iroh_conflict_refresh(refreshed: BoardSummary) {
    // A delayed access response must not restore an already resolved conflict.
    this.boards = this.boards.map((item) =>
      item.id === refreshed.id && item.sync_status === "conflict"
        ? refreshed
        : item,
    );
  }

  iroh_removal_notices = $state<
    Array<{
      board_name: string;
      reason: "SHARE_ACCESS_TERMINATED" | "SHARE_INVITATION_DELETED";
      switched_to: string | null;
    }>
  >([]);

  private iroh_removing = new Set<number>();

  async remove_revoked_iroh_board(board_id: number, error: unknown) {
    const reason = parseCommandError(error)?.code;
    if (
      reason !== "SHARE_ACCESS_TERMINATED" &&
      reason !== "SHARE_INVITATION_DELETED"
    ) {
      return false;
    }
    const shared = this.boards.find((item) => item.id === board_id);
    if (!shared) {
      return true;
    }
    if (shared.shared_role === "owner") {
      return false;
    }
    if (this.iroh_removing.has(board_id)) {
      return true;
    }
    this.iroh_removing.add(board_id);
    const was_active = this.active_board_id === board_id;
    try {
      // Keep the app's last-board invariant without retaining the revoked copy.
      if (
        this.boards.length === 1 &&
        !(await this.create_board(m.share_replacement_board_name()))
      ) {
        return false;
      }
      if (!(await this.delete_board(board_id))) {
        return false;
      }
      delete this.iroh_access_removed[board_id];
      delete this.iroh_access_error[board_id];
      delete this.iroh_sync_error[board_id];
      delete this.iroh_last_synced_at[board_id];
      this.iroh_failures.delete(board_id);
      this.iroh_retry_after.delete(board_id);
      this.iroh_last_checked_at.delete(board_id);
      if (this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      this.iroh_removal_notices.push({
        board_name: shared.name,
        reason,
        switched_to: was_active
          ? (this.boards.find((item) => item.id === this.active_board_id)
              ?.name ?? null)
          : null,
      });
      return true;
    } finally {
      this.iroh_removing.delete(board_id);
    }
  }

  async resolve_iroh_conflict(
    board_id: number,
    keep_local: boolean,
  ): Promise<boolean> {
    try {
      await invoke("resolve_iroh_conflict", {
        boardId: board_id,
        keepLocal: keep_local,
      });
      await this.get_boards();
      this.iroh_failures.delete(board_id);
      this.iroh_retry_after.delete(board_id);
      if (board_id === this.active_board_id && !keep_local) {
        this.can_undo = false;
        this.reload_active_board_data();
      }
      if (this.all_task_list_filter) {
        void this.fetch_all_task_page(true);
      }
      toast.success(
        keep_local ? m.share_local_ready() : m.share_owner_restored(),
      );
      if (keep_local) {
        this.trigger_iroh_background_sync();
      }
      return true;
    } catch (error) {
      logger.error("iroh.conflict_resolve.failed", error);
      if (await this.remove_revoked_iroh_board(board_id, error)) {
        return false;
      }
      toast.error(translateCommandError(error));
      return false;
    }
  }

  /** Called when an owner has accepted an editor's push from another peer. */
  async handle_iroh_remote_push(board_id: number, _revision: number) {
    await this.get_boards();
    if (this.active_board_id === board_id) {
      this.can_undo = false;
      this.reload_active_board_data();
    }
    if (this.all_task_list_filter) {
      void this.fetch_all_task_page(true);
    }
  }

  reconnect_iroh_boards() {
    this.iroh_network_generation += 1;
    this.iroh_failures.clear();
    this.iroh_retry_after.clear();
    this.iroh_sync_error = {};
    this.trigger_iroh_background_sync(true);
  }

  trigger_iroh_background_sync(force = false) {
    if (this.disposed || !this.iroh_enabled) {
      return;
    }
    if (
      typeof document !== "undefined" &&
      document.visibilityState !== "visible"
    ) {
      return;
    }
    if (!this.iroh_host_checking) {
      this.iroh_host_checking = true;
      void invoke("ensure_iroh_host")
        .catch((error) => {
          logger.warn("iroh.host_start.failed", error);
        })
        .finally(() => {
          this.iroh_host_checking = false;
        });
    }
    for (const item of [...this.boards].sort(
      (a, b) =>
        (this.iroh_last_checked_at.get(a.id) ?? 0) -
        (this.iroh_last_checked_at.get(b.id) ?? 0),
    )) {
      if (
        item.shared_role !== "owner" &&
        !this.iroh_syncing.has(item.id) &&
        this.iroh_syncing.size < 2 &&
        Date.now() >= (this.iroh_retry_after.get(item.id) ?? 0)
      ) {
        const access_only =
          item.sync_status === "conflict" ||
          (!force &&
            item.shared_role === "viewer" &&
            Date.now() - (this.iroh_last_synced_at[item.id] ?? 0) <
              IROH_VIEWER_SYNC_MS);
        void this.sync_iroh_board(item.id, true, access_only);
      }
    }
  }

  private update_iroh_summary(board_id: number, sync_status: string) {
    this.boards = this.boards.map((item) =>
      item.id === board_id ? { ...item, sync_status } : item,
    );
  }

  protected start_iroh_background_sync() {
    if (
      this.disposed ||
      this.iroh_sync_timer !== undefined ||
      typeof window === "undefined"
    ) {
      return;
    }
    this.iroh_sync_timer = window.setInterval(
      () => this.trigger_iroh_background_sync(),
      IROH_BACKGROUND_SYNC_MS,
    );
    window.addEventListener("online", this.handle_online);
    document.addEventListener(
      "visibilitychange",
      this.handle_visibility_change,
    );
    this.trigger_iroh_background_sync();
  }

  protected stop_iroh_background_sync() {
    if (this.iroh_sync_timer !== undefined) {
      window.clearInterval(this.iroh_sync_timer);
      this.iroh_sync_timer = undefined;
    }
    if (typeof window !== "undefined") {
      window.removeEventListener("online", this.handle_online);
      document.removeEventListener(
        "visibilitychange",
        this.handle_visibility_change,
      );
    }
  }
}
