import { translateCommandError } from "#lib/command-errors.js";
import { m } from "#lib/paraglide/messages.js";
import { logger } from "#lib/logger.js";
import { invoke } from "@tauri-apps/api/core";
import { untrack } from "svelte";
import { toast } from "svelte-sonner";
import { board } from "./board.svelte";
import {
  build_share_snapshot,
  group_enabled_shares_by_board,
  is_managed_share_enabled,
  publish_share,
  revoke_share,
  resolve_share_selection,
  save_managed_share,
  share_content_signature,
} from "./share";
import { managed_share_state } from "./share-state.svelte";
import { LatestShareSyncQueue } from "./share-sync-queue";
import {
  deserialize_column,
  type ColumnSerialized,
} from "./type/column.svelte";

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
      () => reject(new Error(m.ui_sharing_took_too_long_please_try_again())),
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

// Keeps LAN share links published from a board in step with its content.
export class ManagedShareSync {
  summary = $derived.by(() => {
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

  private readonly queue = new LatestShareSyncQueue<ShareSyncRequest>(
    async (share_id, next, is_current) => {
      if (
        !managed_share_state.shares.some(
          (candidate) => candidate.id === share_id,
        )
      ) {
        return;
      }
      if (
        managed_share_state.published_signatures[share_id] === next.signature
      ) {
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
        new Date(),
        Boolean(next.share.selected_labels?.length),
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
        managed_share_state.shares.some(
          (candidate) => candidate.id === share_id,
        )
      ) {
        save_managed_share(refreshed, next.signature);
      }
    },
    (share_id) => {
      if (
        managed_share_state.shares.some(
          (candidate) => candidate.id === share_id,
        )
      ) {
        managed_share_state.set_sync_state(share_id, "syncing");
      }
    },
    (share_id, error) => {
      if (
        !managed_share_state.shares.some(
          (candidate) => candidate.id === share_id,
        )
      ) {
        return;
      }
      logger.error("share.sync.failed", error);
      managed_share_state.set_sync_state(
        share_id,
        "error",
        m.ui_couldn_t_update_the_shared_board(),
      );
    },
  );

  /** Republishes the active board's links after edits. Call during component initialization. */
  track_active_board() {
    $effect(() => {
      const shares = managed_share_state.shares.filter(
        (share) =>
          is_managed_share_enabled(share) &&
          share.board_id === board.active_board_id,
      );
      if (
        shares.length === 0 ||
        !board.column_fetch_finish ||
        board.column_fetch_error
      ) {
        return;
      }

      const pending: Array<{
        share: (typeof shares)[number];
        signature: string;
      }> = [];
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
            Boolean(share.selected_labels?.length),
          );
          if (
            signature !== managed_share_state.published_signatures[share.id]
          ) {
            pending.push({ share, signature });
          }
        } catch (error) {
          logger.error("share.selection_refresh.failed", error);
          untrack(() =>
            managed_share_state.set_sync_state(
              share.id,
              "error",
              m.ui_shared_content_is_no_longer_available(),
            ),
          );
          void this.disable_stale(share);
        }
      }
      if (pending.length === 0) {
        return;
      }

      const timeout = window.setTimeout(() => {
        // Do not mark the link pending before this callback starts. Svelte may
        // cancel this debounce when the user switches boards; marking it earlier
        // was the source of links stranded at "Updating".
        for (const item of pending) {
          void this.sync(item.share, item.signature).catch(() => undefined);
        }
      }, 700);
      return () => window.clearTimeout(timeout);
    });
  }

  retire(share_id: string) {
    this.queue.retire(share_id);
  }

  sync(
    share: (typeof managed_share_state.shares)[number],
    signature: string,
    columns = board.columns,
  ): Promise<void> {
    if (
      !managed_share_state.shares.some((candidate) => candidate.id === share.id)
    ) {
      return Promise.resolve();
    }
    // The returned promise resolves only after this snapshot (or a later
    // snapshot that superseded it) reaches a terminal state. Board switching
    // therefore cannot outrun a publish already in flight.
    return this.queue.request(share.id, { share, signature, columns });
  }

  async sync_board(board_id: number, columns: typeof board.columns) {
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
          Boolean(share.selected_labels?.length),
        );
        if (signature !== managed_share_state.published_signatures[share.id]) {
          await this.sync(share, signature, columns);
        }
      }),
    );
  }

  async restore_all() {
    const grouped = group_enabled_shares_by_board(managed_share_state.shares);
    await Promise.all(
      [...grouped.entries()].map(async ([board_id, shares]) => {
        try {
          const serialized = await invoke<ColumnSerialized[]>(
            "get_board_columns",
            { boardId: board_id },
          );
          await this.sync_board(board_id, serialized.map(deserialize_column));
        } catch (error) {
          logger.error("share.restore.failed", error);
          for (const share of shares) {
            managed_share_state.set_sync_state(
              share.id,
              "error",
              m.ui_couldn_t_update_the_shared_board(),
            );
          }
          toast.error(m.share_restore_error(), {
            description: translateCommandError(error),
          });
        }
      }),
    );
  }

  private async disable_stale(
    share: (typeof managed_share_state.shares)[number],
  ) {
    try {
      this.queue.retire(share.id);
      await revoke_share(share, "stale_selection");
      const current = managed_share_state.shares.find(
        (candidate) => candidate.id === share.id,
      );
      if (current) {
        save_managed_share({ ...current, enabled: false }, "");
        toast.error(m.share_stopped_named({ name: share.title }));
      }
    } catch (error) {
      logger.error("share.stale_revoke.failed", error);
      if (
        managed_share_state.shares.some(
          (candidate) => candidate.id === share.id,
        )
      ) {
        managed_share_state.set_sync_state(
          share.id,
          "error",
          m.ui_could_not_disable_outdated_share_content_close_cardbe_to_stop_sharing(),
        );
      }
    }
  }

  async flush_active_board() {
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
          Boolean(share.selected_labels?.length),
        );
        if (signature !== managed_share_state.published_signatures[share.id]) {
          await this.sync(share, signature);
        }
      }),
    );
    const failed = shares
      .map((share) => managed_share_state.sync_state(share.id))
      .find((state) => state.status === "error");
    if (failed) {
      throw new Error(m.share_switch_error({ error: failed.error }));
    }
  }
}
