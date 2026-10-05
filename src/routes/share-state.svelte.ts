import { logger } from "$lib/logger";
import type { ManagedShare } from "./share";

const MANAGED_SHARE_STORAGE_KEY = "cardbe.active-share";

interface PersistedManagedShares {
  shares: ManagedShare[];
  /** Legacy single-link format, migrated to `shares` on restore. */
  share?: ManagedShare;
}

export type ShareSyncStatus = "synced" | "pending" | "syncing" | "error";

export interface ShareSyncState {
  status: ShareSyncStatus;
  error: string;
}

function managed_share_storage(): Storage | undefined {
  if (typeof window === "undefined") {
    return undefined;
  }
  return window.localStorage;
}

function is_unexpired(share: ManagedShare): boolean {
  if (!share.expires_at) {
    return true;
  }
  const expiration = Date.parse(share.expires_at);
  return Number.isFinite(expiration) && expiration > Date.now();
}

export class ManagedShareState {
  private restoration_failed = false;
  storage_error = $state<"read" | "write" | null>(null);
  shares = $state<ManagedShare[]>([]);
  published_signatures = $state<Record<string, string>>({});
  sync_states = $state<Record<string, ShareSyncState>>({});

  restore(single_board_id?: number) {
    this.storage_error = null;
    try {
      const storage = managed_share_storage();
      if (!storage) {
        return;
      }
      const value = storage.getItem(MANAGED_SHARE_STORAGE_KEY);
      if (value === null) {
        this.restoration_failed = false;
        this.storage_error = null;
        this.shares = [];
        this.published_signatures = {};
        this.sync_states = {};
        return;
      }

      const persisted = JSON.parse(value) as Partial<PersistedManagedShares>;
      if (
        !persisted ||
        typeof persisted !== "object" ||
        Array.isArray(persisted) ||
        ("shares" in persisted && "share" in persisted) ||
        ("shares" in persisted
          ? !Array.isArray(persisted.shares)
          : !persisted.share)
      ) {
        throw new Error("Invalid managed share storage format");
      }
      const candidates = Array.isArray(persisted.shares)
        ? persisted.shares
        : persisted.share
          ? [persisted.share]
          : [];
      const unique = new Map<string, ManagedShare>();
      for (const share of candidates) {
        if (
          !share ||
          typeof share.id !== "string" ||
          !share.id ||
          unique.has(share.id) ||
          typeof share.url !== "string" ||
          typeof share.title !== "string" ||
          typeof share.updated_at !== "string" ||
          (share.expires_at !== null && typeof share.expires_at !== "string") ||
          !Array.isArray(share.selected_column_ids) ||
          !share.selected_column_ids.every((id) => typeof id === "string") ||
          !Array.isArray(share.selected_task_ids) ||
          !share.selected_task_ids.every((id) => typeof id === "string") ||
          (share.selected_labels !== undefined &&
            (!Array.isArray(share.selected_labels) ||
              !share.selected_labels.every(
                (label) => typeof label === "string",
              ))) ||
          (share.board_id !== undefined && !Number.isInteger(share.board_id)) ||
          (share.enabled !== undefined && typeof share.enabled !== "boolean")
        ) {
          throw new Error("Invalid managed share record");
        }
        if (share?.id) {
          unique.set(share.id, {
            ...share,
            board_id: Number.isInteger(share.board_id)
              ? share.board_id
              : single_board_id,
            // With multiple boards the owner is ambiguous, so keep the
            // link disabled until the user explicitly rebinds it.
            // Expired links also stay configured, but must not be republished.
            enabled:
              is_unexpired(share) &&
              (Number.isInteger(share.board_id) ||
                single_board_id !== undefined)
                ? share.enabled
                : false,
          });
        }
      }

      this.restoration_failed = false;
      const restored = [...unique.values()];
      this.persist(restored);
      this.shares = restored;
      this.published_signatures = {};
      this.sync_states = Object.fromEntries(
        this.shares.map((share) => [
          share.id,
          { status: "pending", error: "" },
        ]),
      );
    } catch (error) {
      this.restoration_failed = true;
      this.storage_error ??= "read";
      logger.warn("share.local_state_restore.failed", error);
    }
  }

  rebind(share_id: string, board_id: number) {
    this.ensure_writable();
    const shares = this.shares.map((share) =>
      share.id === share_id ? { ...share, board_id, enabled: false } : share,
    );
    this.persist(shares);
    this.shares = shares;
  }

  save(share: ManagedShare, signature: string) {
    this.ensure_writable();
    const index = this.shares.findIndex(
      (candidate) => candidate.id === share.id,
    );
    const shares =
      index === -1
        ? [...this.shares, share]
        : this.shares.map((candidate, candidate_index) =>
            candidate_index === index ? share : candidate,
          );
    this.persist(shares);
    this.shares = shares;
    this.published_signatures = {
      ...this.published_signatures,
      [share.id]: signature,
    };
    this.set_sync_state(share.id, "synced");
  }

  forget(share_id: string) {
    this.ensure_writable();
    const shares = this.shares.filter((share) => share.id !== share_id);
    this.persist(shares);
    this.shares = shares;
    const { [share_id]: _signature, ...published_signatures } =
      this.published_signatures;
    const { [share_id]: _sync_state, ...sync_states } = this.sync_states;
    this.published_signatures = published_signatures;
    this.sync_states = sync_states;
  }

  set_sync_state(share_id: string, status: ShareSyncStatus, error = "") {
    this.sync_states = {
      ...this.sync_states,
      [share_id]: { status, error },
    };
  }

  sync_state(share_id: string): ShareSyncState {
    return this.sync_states[share_id] ?? { status: "pending", error: "" };
  }

  ensure_writable() {
    if (this.restoration_failed) {
      throw new Error("Managed share storage must be recovered before writing");
    }
  }

  private persist(shares: ManagedShare[]) {
    this.ensure_writable();
    try {
      const storage = managed_share_storage();
      if (!storage) {
        return;
      }
      if (shares.length === 0) {
        storage.removeItem(MANAGED_SHARE_STORAGE_KEY);
      } else {
        const value: PersistedManagedShares = { shares };
        storage.setItem(MANAGED_SHARE_STORAGE_KEY, JSON.stringify(value));
      }
      this.storage_error = null;
    } catch (error) {
      this.storage_error = "write";
      throw error;
    }
  }
}

export const managed_share_state = new ManagedShareState();

export function restore_managed_share_state(single_board_id?: number): void {
  managed_share_state.restore(single_board_id);
}
