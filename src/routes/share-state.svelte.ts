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
    if (typeof window === "undefined") return undefined;
    return window.localStorage;
}

function is_unexpired(share: ManagedShare): boolean {
    if (!share.expires_at) return true;
    const expiration = Date.parse(share.expires_at);
    return Number.isFinite(expiration) && expiration > Date.now();
}

export class ManagedShareState {
    shares = $state<ManagedShare[]>([]);
    published_signatures = $state<Record<string, string>>({});
    sync_states = $state<Record<string, ShareSyncState>>({});

    restore(single_board_id?: number) {
        const storage = managed_share_storage();
        if (!storage) return;

        try {
            const value = storage.getItem(MANAGED_SHARE_STORAGE_KEY);
            if (!value) return;

            const persisted = JSON.parse(value) as Partial<PersistedManagedShares>;
            const candidates = Array.isArray(persisted.shares)
                ? persisted.shares
                : persisted.share ? [persisted.share] : [];
            const unique = new Map<string, ManagedShare>();
            for (const share of candidates) {
                if (share?.id && is_unexpired(share)) unique.set(share.id, {
                    ...share,
                    board_id: Number.isInteger(share.board_id) ? share.board_id : single_board_id,
                    // With multiple boards the owner is ambiguous, so keep the
                    // link disabled until the user explicitly rebinds it.
                    enabled: Number.isInteger(share.board_id) || single_board_id !== undefined ? share.enabled : false,
                });
            }

            this.shares = [...unique.values()];
            this.published_signatures = {};
            this.sync_states = Object.fromEntries(
                this.shares.map((share) => [share.id, { status: "pending", error: "" }]),
            );
            this.persist();
        } catch {
            storage.removeItem(MANAGED_SHARE_STORAGE_KEY);
        }
    }

    rebind(share_id: string, board_id: number) {
        this.shares = this.shares.map((share) => share.id === share_id
            ? { ...share, board_id, enabled: false }
            : share);
        this.persist();
    }

    save(share: ManagedShare, signature: string) {
        const index = this.shares.findIndex((candidate) => candidate.id === share.id);
        this.shares = index === -1
            ? [...this.shares, share]
            : this.shares.map((candidate, candidate_index) => candidate_index === index ? share : candidate);
        this.published_signatures = { ...this.published_signatures, [share.id]: signature };
        this.set_sync_state(share.id, "synced");
        this.persist();
    }

    forget(share_id: string) {
        this.shares = this.shares.filter((share) => share.id !== share_id);
        const { [share_id]: _signature, ...published_signatures } = this.published_signatures;
        const { [share_id]: _sync_state, ...sync_states } = this.sync_states;
        this.published_signatures = published_signatures;
        this.sync_states = sync_states;
        this.persist();
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

    private persist() {
        const storage = managed_share_storage();
        if (!storage) return;
        if (this.shares.length === 0) {
            storage.removeItem(MANAGED_SHARE_STORAGE_KEY);
            return;
        }

        const value: PersistedManagedShares = { shares: this.shares };
        storage.setItem(MANAGED_SHARE_STORAGE_KEY, JSON.stringify(value));
    }
}

export const managed_share_state = new ManagedShareState();

export function restore_managed_share_state(single_board_id?: number): void {
    managed_share_state.restore(single_board_id);
}
