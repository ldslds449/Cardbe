import { afterEach, describe, expect, it, vi } from "vitest";
import type { ManagedShare } from "./share";
import { ManagedShareState } from "./share-state.svelte";

const STORAGE_KEY = "cardbe.active-share";

function memory_storage(): Storage {
    const values = new Map<string, string>();
    return {
        get length() {
            return values.size;
        },
        clear: () => values.clear(),
        getItem: (key) => values.get(key) ?? null,
        key: (index) => [...values.keys()][index] ?? null,
        removeItem: (key) => values.delete(key),
        setItem: (key, value) => values.set(key, value),
    };
}

describe("managed share restoration", () => {
    afterEach(() => vi.unstubAllGlobals());

    it("durably restores a LAN share and forces it to be republished after restart", () => {
        const local_storage = memory_storage();
        const share: ManagedShare = {
            id: "j3V_BXrcsGwtsXv6XAD1jA",
            url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
            updated_at: "2026-09-01T12:00:00Z",
            expires_at: null,
            title: "Roadmap",
            selected_column_ids: ["column_1"],
            selected_task_ids: ["task_1"],
            board_id: 1,
        };
        local_storage.setItem(STORAGE_KEY, JSON.stringify({ shares: [share] }));
        vi.stubGlobal("window", { localStorage: local_storage });

        const state = new ManagedShareState();
        state.restore();

        expect(state.shares).toEqual([share]);
        expect(local_storage.getItem(STORAGE_KEY)).not.toContain("published-content");
        expect(state.published_signatures).toEqual({});
        expect(state.sync_state(share.id).status).toBe("pending");
        state.save(share, "private card description");
        expect(local_storage.getItem(STORAGE_KEY)).not.toContain("private card description");
    });


    it("migrates a legacy single-share record without dropping the link", () => {
        const local_storage = memory_storage();
        const share: ManagedShare = {
            id: "j3V_BXrcsGwtsXv6XAD1jA",
            url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
            updated_at: "2026-09-01T12:00:00Z",
            expires_at: null,
            title: "Roadmap",
            selected_column_ids: ["column_1"],
            selected_task_ids: ["task_1"],
        };
        local_storage.setItem(STORAGE_KEY, JSON.stringify({ share }));
        vi.stubGlobal("window", { localStorage: local_storage });

        const state = new ManagedShareState();
        state.restore();

        expect(state.shares).toEqual([{ ...share, enabled: false }]);
        expect(JSON.parse(local_storage.getItem(STORAGE_KEY)!)).toEqual({ shares: [{ ...share, enabled: false }] });
        expect(state.sync_state(share.id).status).toBe("pending");
    });

    it("automatically binds a legacy share when there is only one board", () => {
        const local_storage = memory_storage();
        const share = {
            id: "j3V_BXrcsGwtsXv6XAD1jA", url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
            updated_at: "2026-09-01T12:00:00Z", expires_at: null, title: "Roadmap",
            selected_column_ids: [], selected_task_ids: [],
        } as ManagedShare;
        local_storage.setItem(STORAGE_KEY, JSON.stringify({ shares: [share] }));
        vi.stubGlobal("window", { localStorage: local_storage });
        const state = new ManagedShareState();
        state.restore(42);
        expect(state.shares[0].board_id).toBe(42);
        expect(state.shares[0].enabled).not.toBe(false);
    });

    it("lets a user explicitly assign an ambiguous legacy share", () => {
        const local_storage = memory_storage();
        const share = {
            id: "j3V_BXrcsGwtsuYw7YBE2kB", url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsuYw7YBE2kB",
            updated_at: "2026-09-01T12:00:00Z", expires_at: null, title: "Legacy",
            selected_column_ids: [], selected_task_ids: [],
        } as ManagedShare;
        local_storage.setItem(STORAGE_KEY, JSON.stringify({ shares: [share] }));
        vi.stubGlobal("window", { localStorage: local_storage });
        const state = new ManagedShareState();
        state.restore();
        state.rebind(share.id, 9);
        expect(state.shares[0]).toMatchObject({ board_id: 9, enabled: false });
        expect(local_storage.getItem(STORAGE_KEY)).toContain('"board_id":9');
    });

    it("does not restore an expired durable share", () => {
        const local_storage = memory_storage();
        local_storage.setItem(STORAGE_KEY, JSON.stringify({
            shares: [{
                id: "j3V_BXrcsGwtsXv6XAD1jA",
                url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
                updated_at: "2026-09-01T12:00:00Z",
                expires_at: "2000-01-01T00:00:00Z",
                title: "Roadmap",
                selected_column_ids: ["column_1"],
                selected_task_ids: ["task_1"],
            }],
        }));
        vi.stubGlobal("window", { localStorage: local_storage });

        const state = new ManagedShareState();
        state.restore();

        expect(state.shares).toEqual([]);
        expect(local_storage.getItem(STORAGE_KEY)).toBeNull();
    });

    it("persists and independently removes multiple shares", () => {
        const local_storage = memory_storage();
        vi.stubGlobal("window", { localStorage: local_storage });
        const first: ManagedShare = {
            id: "j3V_BXrcsGwtsXv6XAD1jA",
            url: "http://192.168.1.20:12345/share/j3V_BXrcsGwtsXv6XAD1jA",
            updated_at: "2026-09-01T12:00:00Z",
            expires_at: null,
            title: "Roadmap",
            selected_column_ids: ["column_1"],
            selected_task_ids: ["task_1"],
            board_id: 1,
        };
        const second = {
            ...first,
            id: "k4W_ACyrtHxtuYw7YBE2kB",
            url: "http://192.168.1.20:12345/share/k4W_ACyrtHxtuYw7YBE2kB",
            title: "Support",
            enabled: false,
        };
        const state = new ManagedShareState();

        state.save(first, "first-signature");
        state.save(second, "second-signature");

        expect(state.shares).toEqual([first, second]);
        expect(local_storage.getItem(STORAGE_KEY)).toContain(first.id);
        expect(local_storage.getItem(STORAGE_KEY)).toContain(second.id);
        expect(local_storage.getItem(STORAGE_KEY)).not.toContain("signature");

        const restored = new ManagedShareState();
        restored.restore();
        expect(restored.shares).toEqual([first, second]);
        expect(restored.sync_state(first.id).status).toBe("pending");
        expect(restored.sync_state(second.id).status).toBe("pending");

        restored.forget(first.id);
        expect(restored.shares).toEqual([second]);
        expect(local_storage.getItem(STORAGE_KEY)).not.toContain(first.id);
        expect(local_storage.getItem(STORAGE_KEY)).toContain(second.id);
    });
});
