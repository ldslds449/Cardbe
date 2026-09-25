import type { Column } from "./type/column.svelte";
import { serialize_task, type TaskSerialized } from "./type/task.svelte";
import { invoke } from "@tauri-apps/api/core";
import { managed_share_state } from "./share-state.svelte";

export const SHARE_SCHEMA_VERSION = 1;
export const MAX_SHARE_TITLE_LENGTH = 120;
export const MAX_SHARED_COLUMNS = 50;
export const MAX_SHARED_TASKS = 2_000;

export interface SharedColumn {
    id: number;
    name: string;
    color: string;
    tasks: TaskSerialized[];
}

export interface ShareSnapshot {
    schema_version: number;
    title: string;
    published_at: string;
    columns: SharedColumn[];
}

export interface ManagedShare {
    id: string;
    url: string;
    updated_at: string;
    expires_at: string | null;
    title: string;
    selected_column_ids: string[];
    selected_task_ids: string[];
    /** Cards carrying any of these labels are included as the board changes. */
    selected_labels?: string[];
    /** Disabled links stay configured locally but are not served on the LAN. */
    enabled?: boolean;
    /** Missing only on legacy links; such links must not be automatically served. */
    board_id?: number;
}

export interface ShareSelection {
    selected_column_ids: string[];
    selected_task_ids: string[];
}

export interface RequestedShareLink {
    id: string;
    preferred_port: number | null;
}

const SHARE_ID_PATTERN = /^[A-Za-z0-9_-]{22,64}$/;

/** Resolve manual selections and dynamic label rules against the current board. */
export function resolve_share_selection(
    columns: Column[],
    selected_column_ids: string[],
    selected_task_ids: string[],
    selected_labels: string[] = [],
): ShareSelection {
    const labels = new Set(selected_labels);
    const task_ids = new Set(selected_task_ids);
    const column_ids = new Set(selected_column_ids);

    if (labels.size > 0) {
        for (const column of columns) {
            for (const task of column.tasks) {
                if (task.labels.some((label) => labels.has(label))) {
                    task_ids.add(task.id);
                    column_ids.add(column.id);
                }
            }
        }
    }

    return { selected_column_ids: [...column_ids], selected_task_ids: [...task_ids] };
}

function numeric_id(value: string, prefix: string): number {
    const match = value.match(new RegExp(`^${prefix}_(\\d+)$`));
    if (!match) throw new Error(`Invalid ${prefix} ID: ${value}`);
    return Number.parseInt(match[1], 10);
}

export function build_share_snapshot(
    columns: Column[],
    selected_column_ids: string[],
    selected_task_ids: string[],
    title: string,
    now = new Date(),
): ShareSnapshot {
    const normalized_title = title.trim();
    if (!normalized_title) throw new Error("Enter a name for the shared board");
    if (normalized_title.length > MAX_SHARE_TITLE_LENGTH) {
        throw new Error(`Board name must be ${MAX_SHARE_TITLE_LENGTH} characters or fewer`);
    }

    const selected = new Set(selected_column_ids);
    const selected_tasks = new Set(selected_task_ids);
    const shared_columns = columns
        .filter((column) => selected.has(column.id))
        .map((column) => ({
            id: numeric_id(column.id, "column"),
            name: column.name,
            color: column.color,
            tasks: column.tasks.filter((task) => selected_tasks.has(task.id)).map(serialize_task),
        }));

    if (shared_columns.length === 0) throw new Error("Select at least one column to share");
    if (shared_columns.length > MAX_SHARED_COLUMNS) {
        throw new Error(`A share can contain at most ${MAX_SHARED_COLUMNS} columns`);
    }
    const task_count = shared_columns.reduce((total, column) => total + column.tasks.length, 0);
    if (task_count > MAX_SHARED_TASKS) {
        throw new Error(`A share can contain at most ${MAX_SHARED_TASKS} tasks`);
    }

    return {
        schema_version: SHARE_SCHEMA_VERSION,
        title: normalized_title,
        published_at: now.toISOString(),
        columns: shared_columns,
    };
}

export function share_content_signature(
    columns: Column[],
    selected_column_ids: string[],
    selected_task_ids: string[],
    title: string,
): string {
    return JSON.stringify(
        build_share_snapshot(
            columns,
            selected_column_ids,
            selected_task_ids,
            title,
            new Date(0),
        ),
    );
}

function create_share_id(): string {
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

/** Parse either a capability ID or a previously published Cardbe LAN URL. */
export function parse_requested_share_link(value: string): RequestedShareLink {
    const normalized = value.trim();
    if (SHARE_ID_PATTERN.test(normalized)) {
        return { id: normalized, preferred_port: null };
    }

    let url: URL;
    try {
        url = new URL(normalized);
    } catch {
        throw new Error("Enter a valid Cardbe share link or share ID");
    }

    const match = url.pathname.match(/^\/share\/([A-Za-z0-9_-]{22,64})$/);
    const port = Number.parseInt(url.port, 10);
    if (
        url.protocol !== "http:" ||
        url.username ||
        url.password ||
        url.search ||
        url.hash ||
        !match ||
        !Number.isInteger(port) ||
        port < 1 ||
        port > 65_535
    ) {
        throw new Error("Enter a valid Cardbe share link or share ID");
    }

    return { id: match[1], preferred_port: port };
}

function existing_share_port(share: ManagedShare | null): number | null {
    if (!share) return null;
    try {
        const port = Number.parseInt(new URL(share.url).port, 10);
        return Number.isInteger(port) && port >= 1 && port <= 65_535 ? port : null;
    } catch {
        return null;
    }
}

export function load_managed_shares(): ManagedShare[] {
    return managed_share_state.shares;
}

export function group_enabled_shares_by_board(shares: ManagedShare[]): Map<number, ManagedShare[]> {
    const grouped = new Map<number, ManagedShare[]>();
    for (const share of shares) {
        if (!is_managed_share_enabled(share) || share.board_id === undefined) continue;
        grouped.set(share.board_id, [...(grouped.get(share.board_id) ?? []), share]);
    }
    return grouped;
}

export function is_managed_share_enabled(share: ManagedShare): boolean {
    return share.enabled !== false && Number.isInteger(share.board_id);
}

export function save_managed_share(share: ManagedShare, signature: string): void {
    managed_share_state.save(share, signature);
}

export function forget_managed_share(share_id: string): void {
    managed_share_state.forget(share_id);
}

export function rebind_managed_share(share_id: string, board_id: number): void {
    managed_share_state.rebind(share_id, board_id);
}

export async function publish_share(
    snapshot: ShareSnapshot,
    selected_column_ids: string[],
    selected_task_ids: string[],
    expires_at: Date | null,
    existing: ManagedShare | null,
    selected_labels: string[] = [],
    requested_link: string | null = null,
    reactivate = false,
    board_id?: number,
): Promise<ManagedShare> {
    const requested = !existing && requested_link?.trim()
        ? parse_requested_share_link(requested_link)
        : null;
    const result = await invoke<{
        id: string;
        url: string;
        updated_at: string;
        expires_at: string | null;
    }>("publish_lan_share", {
        boardId: board_id ?? existing?.board_id ?? null,
        snapshot,
        expiresAt: expires_at?.toISOString() ?? null,
        shareId: existing?.id ?? requested?.id ?? create_share_id(),
        // A retired ID may be restored only through this explicit user action.
        // Automatic updates keep this false so a delayed update cannot undo revoke.
        allowReactivate: requested !== null || reactivate,
        // Rebind the previous listener port after a backend/computer restart so
        // the complete URL, not just its capability ID, remains reusable.
        preferredPort: existing_share_port(existing) ?? requested?.preferred_port ?? null,
    });
    return {
        ...result,
        title: snapshot.title,
        selected_column_ids,
        selected_task_ids,
        selected_labels,
        enabled: true,
        board_id: board_id ?? existing?.board_id,
    };
}

export async function revoke_share(share: ManagedShare): Promise<void> {
    await invoke("revoke_lan_share", { shareId: share.id });
}

export async function revoke_managed_shares(
    predicate: (share: ManagedShare) => boolean,
): Promise<void> {
    const targets = [...managed_share_state.shares].filter(predicate);
    for (const share of targets) {
        await revoke_share(share);
        managed_share_state.forget(share.id);
    }
}
