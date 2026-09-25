use crate::models::{
    Archive, Board, Column, ColumnSort, Note, StoredData, Task, TaskTemplate,
    CURRENT_SCHEMA_VERSION,
};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type StorageResult<T> = Result<T, Box<dyn std::error::Error>>;

pub struct LoadedData {
    pub stored: StoredData,
    pub database: Database,
    pub recovery_messages: Vec<String>,
    pub archives_loaded: bool,
}

pub struct Database {
    connection: Connection,
    path: PathBuf,
    positions: DatabasePositions,
    active_board_id: i64,
}

#[derive(Clone, Default)]
struct DatabasePositions {
    columns: HashMap<i64, i64>,
    tasks: HashMap<i64, i64>,
}

const SORT_POSITION_GAP: i64 = 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PersistStats {
    pub columns_changed: usize,
    pub tasks_changed: usize,
    pub archives_changed: usize,
    pub templates_changed: usize,
    pub metadata_changed: usize,
}

impl Database {
    pub(crate) fn open(path: PathBuf) -> StorageResult<Self> {
        let connection = Connection::open(&path)?;
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS columns (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 color TEXT NOT NULL,
                 position INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS tasks (
                 id INTEGER PRIMARY KEY,
                 column_id INTEGER NOT NULL REFERENCES columns(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL,
                 payload TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS tasks_by_column_position
                 ON tasks(column_id, position);
             CREATE TABLE IF NOT EXISTS archives (
                 task_id INTEGER PRIMARY KEY,
                 archived_at TEXT NOT NULL,
                 position INTEGER NOT NULL,
                 payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS templates (
                 id INTEGER PRIMARY KEY,
                 name TEXT NOT NULL,
                 position INTEGER NOT NULL,
                 payload TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS notes (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 title TEXT NOT NULL DEFAULT '',
                 content TEXT NOT NULL DEFAULT '',
                 pinned INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS notes_by_pinned_updated
                 ON notes(pinned DESC, updated_at DESC);
             CREATE TABLE IF NOT EXISTS metadata (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             ",
        )?;
        let has_sort_order = connection
            .prepare("PRAGMA table_info(columns)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == "sort_order");
        if !has_sort_order {
            connection.execute(
                "ALTER TABLE columns ADD COLUMN sort_order TEXT NOT NULL DEFAULT 'custom'",
                [],
            )?;
        }
        let positions = DatabasePositions::default();
        Ok(Self {
            connection,
            path,
            positions,
            active_board_id: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn active_board_id(&self) -> i64 {
        self.active_board_id
    }

    pub fn get_notes(&self) -> StorageResult<Vec<Note>> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, content, pinned, created_at, updated_at
             FROM notes ORDER BY pinned DESC, updated_at DESC, id DESC",
        )?;
        let notes = statement
            .query_map([], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(notes)
    }

    pub fn create_note(&mut self, now: i64) -> StorageResult<Note> {
        self.create_note_with_content(now, String::new(), String::new())
    }

    pub fn boards(&self) -> StorageResult<Vec<Board>> {
        Ok(self
            .connection
            .prepare(
                "SELECT b.id, b.name, COUNT(t.id), b.shared_role, b.sync_status, b.sync_revision
                 FROM cardbe_boards b
                 LEFT JOIN cardbe_tasks t ON t.board_id = b.id
                 GROUP BY b.id, b.name, b.shared_role, b.sync_status, b.sync_revision
                 ORDER BY b.id",
            )?
            .query_map([], |row| {
                Ok(Board {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    task_count: row.get(2)?,
                    shared_role: row.get(3)?,
                    sync_status: row.get(4)?,
                    sync_revision: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn initialize_boards(
        &mut self,
        legacy: &StoredData,
    ) -> StorageResult<(Vec<Board>, StoredData)> {
        self.migrate_multiboard_schema(legacy)?;
        let boards = self.boards()?;
        if let Some(board) = boards.first() {
            let id = namespaced_metadata_parse::<i64>(&self.connection, "active_board_id")?
                .filter(|id| boards.iter().any(|board| board.id == *id))
                .unwrap_or(board.id);
            let stored = self.load_board(id)?;
            return Ok((boards, stored));
        }
        self.connection
            .execute("INSERT INTO cardbe_boards(name) VALUES ('My board')", [])?;
        let id = self.connection.last_insert_rowid();
        self.write_board(id, legacy)?;
        self.set_active_board_id(id)?;
        Ok((
            vec![Board {
                id,
                name: "My board".into(),
                task_count: legacy
                    .columns
                    .iter()
                    .map(|column| column.tasks.len() as i64)
                    .sum(),
                shared_role: "owner".into(),
                sync_status: "local".into(),
                sync_revision: 0,
            }],
            legacy.clone(),
        ))
    }

    pub fn load_board(&mut self, id: i64) -> StorageResult<StoredData> {
        let stored = self.load_board_data(id, true)?;
        self.positions = self.load_board_positions(id)?;
        self.set_active_board_id(id)?;
        Ok(stored)
    }

    /// Read a board without changing the active-board metadata or cached positions.
    pub fn read_board(&self, id: i64) -> StorageResult<StoredData> {
        self.load_board_data(id, false)
    }

    /// Read every board-owned collection without changing active state.
    pub fn read_board_complete(&self, id: i64) -> StorageResult<StoredData> {
        self.load_board_data(id, true)
    }

    pub fn read_board_share_snapshot(&self, id: i64) -> StorageResult<(Board, StoredData)> {
        self.connection
            .execute_batch("BEGIN DEFERRED TRANSACTION")?;
        let result = (|| {
            let board = self
                .boards()?
                .into_iter()
                .find(|board| board.id == id)
                .ok_or("Board was deleted")?;
            let data = self.load_board_data(id, true)?;
            Ok((board, data))
        })();
        self.connection.execute_batch("ROLLBACK")?;
        result
    }

    pub fn load_board_archives_for_export(&self, id: i64) -> StorageResult<Vec<Archive>> {
        self.load_board_archives(id)
    }

    pub fn replace_all_boards(
        &mut self,
        boards: &[(String, StoredData)],
        active_index: usize,
        notes: &[Note],
        settings: &crate::models::Settings,
    ) -> StorageResult<(Vec<Board>, StoredData)> {
        if boards.is_empty() || active_index >= boards.len() {
            return Err("Backup must contain an active board".into());
        }
        let tx = self.connection.transaction()?;
        tx.execute("DELETE FROM cardbe_boards", [])?;
        tx.execute("DELETE FROM notes", [])?;
        let mut created = Vec::with_capacity(boards.len());
        for (name, data) in boards {
            tx.execute("INSERT INTO cardbe_boards(name) VALUES(?1)", [name])?;
            let id = tx.last_insert_rowid();
            write_board_transaction(&tx, id, data)?;
            created.push(Board {
                id,
                name: name.clone(),
                task_count: data
                    .columns
                    .iter()
                    .map(|column| column.tasks.len() as i64)
                    .sum(),
                shared_role: "owner".into(),
                sync_status: "local".into(),
                sync_revision: 0,
            });
        }
        for note in notes {
            tx.execute("INSERT INTO notes(id,title,content,pinned,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)", params![note.id, note.title, note.content, note.pinned, note.created_at, note.updated_at])?;
        }
        let active_id = created[active_index].id;
        tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES('active_board_id',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [active_id.to_string()])?;
        tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES('settings',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(settings)?])?;
        tx.commit()?;
        self.active_board_id = active_id;
        self.positions = self.load_board_positions(active_id)?;
        let active = self.load_board_data(active_id, true)?;
        Ok((created, active))
    }

    pub fn create_board(&mut self, name: &str) -> StorageResult<Board> {
        // Board creation writes both the row and its scoped metadata. Keep
        // those writes in the same transaction so a storage error cannot
        // leave a selectable, half-initialized board behind.
        self.create_board_with_data(name, &StoredData::default())
    }

    pub fn board_exists(&self, id: i64) -> StorageResult<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM cardbe_boards WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?)
    }

    pub fn board_role(&self, id: i64) -> StorageResult<String> {
        Ok(self.connection.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [id],
            |row| row.get(0),
        )?)
    }

    #[cfg(test)]
    pub fn set_shared_board(
        &mut self,
        id: i64,
        role: &str,
        status: &str,
        revision: i64,
    ) -> StorageResult<()> {
        if self.connection.execute(
            "UPDATE cardbe_boards SET shared_role=?2,sync_status=?3,sync_revision=?4 WHERE id=?1",
            params![id, role, status, revision],
        )? == 0
        {
            return Err("Board not found".into());
        }
        Ok(())
    }

    /// Iroh capabilities are deliberately database-only metadata.  The JSON
    /// backup format remains portable and must never contain bearer secrets.
    pub fn save_iroh_invite(
        &mut self,
        invite_id: &str,
        board_id: i64,
        secret: &str,
        permission: &str,
    ) -> StorageResult<()> {
        self.connection.execute("INSERT INTO cardbe_iroh_invites(invite_id,board_id,secret,permission,enabled,created_at) VALUES(?1,?2,?3,?4,1,datetime('now')) ON CONFLICT(invite_id) DO UPDATE SET board_id=excluded.board_id,secret=excluded.secret,permission=excluded.permission", params![invite_id, board_id, secret, permission])?; // gitleaks:allow
        Ok(())
    }
    pub fn iroh_remote(&self, board_id: i64) -> StorageResult<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT ticket FROM cardbe_iroh_remotes WHERE board_id=?1",
                [board_id],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn iroh_loro_update(&self, board_id: i64) -> StorageResult<Option<Vec<u8>>> {
        Ok(self
            .connection
            .query_row(
                "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
                [board_id],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn ensure_iroh_loro_doc(&mut self, board_id: i64) -> StorageResult<Vec<u8>> {
        if let Some(update) = self.iroh_loro_update(board_id)? {
            return Ok(update);
        }
        let data = self.read_board_complete(board_id)?;
        let update = crate::loro_board::apply_local_delta(None, &StoredData::default(), &data)
            .map_err(|e| format!("Could not seed shared-board CRDT: {e}"))?;
        self.connection.execute(
            "INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2)",
            params![board_id, update],
        )?;
        self.iroh_loro_update(board_id)?
            .ok_or_else(|| "Could not load seeded shared-board CRDT".into())
    }
    /// Applies an editor's Loro update atomically with the SQL projection.
    /// The capability is checked inside this transaction so revocation wins
    /// even when it races an already accepted Iroh connection.
    pub fn apply_iroh_loro_update(
        &mut self,
        board_id: i64,
        invite_id: &str,
        secret: &str,
        update: &[u8],
    ) -> StorageResult<(bool, i64, StoredData)> {
        let previous = self.read_board_complete(board_id)?;
        let tx = self.connection.transaction()?;
        let authorized: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM cardbe_iroh_invites WHERE invite_id=?1 AND board_id=?2 AND secret=?3 AND permission='editor' AND enabled=1)",
            params![invite_id, board_id, secret], |row| row.get(0),
        )?;
        if !authorized {
            return Err("Invitation no longer grants editing access".into());
        }
        let existing: Option<Vec<u8>> = tx
            .query_row(
                "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
                [board_id],
                |row| row.get(0),
            )
            .optional()?;
        let existing = existing.ok_or("Editable board is missing its Loro document")?;
        let merged = crate::loro_board::merge(Some(&existing), update)
            .map_err(|e| format!("Invalid Loro update: {e}"))?;
        let revision: i64 = tx.query_row(
            "SELECT sync_revision FROM cardbe_boards WHERE id=?1 AND shared_role='owner'",
            [board_id],
            |row| row.get(0),
        )?;
        // Snapshot bytes can differ after re-export even when the Loro
        // operation history is unchanged. Use its version vector for retries.
        let unchanged = crate::loro_board::state_vector(Some(&existing))
            .map_err(|e| format!("Invalid stored Loro document: {e}"))?
            == crate::loro_board::state_vector(Some(&merged))
                .map_err(|e| format!("Invalid merged Loro document: {e}"))?;
        if unchanged {
            tx.commit()?;
            return Ok((false, revision, previous));
        }
        let projected = crate::loro_board::project(&merged)
            .map_err(|e| format!("Invalid Loro projection: {e}"))?;
        let mut data = previous;
        data.columns = projected.columns;
        data.archives = projected.archives;
        data.templates = projected.templates;
        write_board_transaction(&tx, board_id, &data)?;
        tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, merged])?;
        tx.execute("UPDATE cardbe_boards SET sync_revision=sync_revision+1,sync_status='synced' WHERE id=?1", [board_id])?;
        tx.commit()?;
        Ok((true, revision + 1, data))
    }
    /// Client-side counterpart: merge an owner-approved diff into the local
    /// document and atomically refresh only the CRDT-owned projection.
    pub fn merge_iroh_loro_diff(
        &mut self,
        board_id: i64,
        update: &[u8],
        name: &str,
        role: &str,
        revision: i64,
        sent: &[u8],
    ) -> StorageResult<StoredData> {
        let previous = self.read_board_complete(board_id)?;
        let current = self
            .iroh_loro_update(board_id)?
            .ok_or("Editable board is missing its Loro document")?;
        let changed_during_sync = current != sent;
        let merged = crate::loro_board::merge(Some(&current), update)
            .map_err(|e| format!("Invalid Loro update: {e}"))?;
        let projected = crate::loro_board::project(&merged)
            .map_err(|e| format!("Invalid Loro projection: {e}"))?;
        let mut data = previous;
        data.columns = projected.columns;
        data.archives = projected.archives;
        data.templates = projected.templates;
        let tx = self.connection.transaction()?;
        write_board_transaction(&tx, board_id, &data)?;
        tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, merged])?;
        tx.execute("UPDATE cardbe_boards SET name=?2,shared_role=?3,sync_revision=?4,sync_status=?5 WHERE id=?1", params![board_id, name, role, revision, if changed_during_sync { "pending" } else { "synced" }])?;
        tx.commit()?;
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(data)
    }

    pub fn save_iroh_conflict(
        &mut self,
        board_id: i64,
        revision: i64,
        name: &str,
        permission: &str,
        data: &StoredData,
    ) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO cardbe_iroh_conflicts(board_id,revision,name,permission,payload) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(board_id) DO UPDATE SET revision=excluded.revision,name=excluded.name,permission=excluded.permission,payload=excluded.payload", params![board_id, revision, name, permission, serde_json::to_string(data)?])?;
        tx.execute(
            "UPDATE cardbe_boards SET sync_status='conflict',shared_role=?2 WHERE id=?1",
            params![board_id, permission],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn iroh_conflict(
        &self,
        board_id: i64,
    ) -> StorageResult<Option<(i64, String, String, StoredData)>> {
        let raw: Option<(i64, String, String, String)> = self.connection.query_row(
            "SELECT revision,name,permission,payload FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).optional()?;
        raw.map(|(revision, name, permission, payload)| {
            Ok((revision, name, permission, serde_json::from_str(&payload)?))
        })
        .transpose()
    }

    pub fn keep_iroh_conflict_local(&mut self, board_id: i64, revision: i64) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        tx.execute("UPDATE cardbe_boards SET shared_role='editor',sync_status='pending',sync_revision=?2 WHERE id=?1", params![board_id, revision])?;
        tx.execute(
            "DELETE FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn board_sync_state(&self, id: i64) -> StorageResult<(i64, String)> {
        Ok(self.connection.query_row(
            "SELECT sync_revision,sync_status FROM cardbe_boards WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }
    pub fn iroh_remote_tickets(&self) -> StorageResult<Vec<String>> {
        Ok(self
            .connection
            .prepare("SELECT ticket FROM cardbe_iroh_remotes")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn iroh_invites(&self) -> StorageResult<Vec<(String, i64, String, String, bool)>> {
        Ok(self
            .connection
            .prepare(
                "SELECT invite_id,board_id,secret,permission,enabled FROM cardbe_iroh_invites",
            )?
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn iroh_invite_summaries(
        &self,
    ) -> StorageResult<Vec<(String, i64, String, bool, String, String)>> {
        Ok(self.connection.prepare("SELECT invite_id,board_id,permission,enabled,created_at,secret FROM cardbe_iroh_invites ORDER BY created_at DESC")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0, r.get(4)?, r.get(5)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }
    pub fn update_iroh_invite(
        &mut self,
        invite_id: &str,
        permission: Option<&str>,
        enabled: Option<bool>,
    ) -> StorageResult<()> {
        if self.connection.execute(
            "UPDATE cardbe_iroh_invites SET permission=COALESCE(?2,permission),enabled=COALESCE(?3,enabled) WHERE invite_id=?1",
            params![invite_id, permission, enabled.map(i64::from)],
        )? == 0 {
            return Err("Invitation not found".into());
        }
        Ok(())
    }
    pub fn delete_iroh_invite(&mut self, invite_id: &str) -> StorageResult<()> {
        if self.connection.execute(
            "DELETE FROM cardbe_iroh_invites WHERE invite_id=?1",
            [invite_id],
        )? == 0
        {
            return Err("Invitation not found".into());
        }
        Ok(())
    }
    pub fn iroh_endpoint_seed(&mut self) -> StorageResult<String> {
        if let Some(seed) = self
            .connection
            .query_row(
                "SELECT value FROM cardbe_global_metadata WHERE key='iroh_endpoint_seed'",
                [],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(seed);
        }
        let seed = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(iroh::SecretKey::generate().to_bytes());
        self.connection.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES('iroh_endpoint_seed',?1)",
            [&seed],
        )?;
        Ok(seed)
    }
    pub fn apply_iroh_snapshot(
        &mut self,
        board_id: i64,
        name: &str,
        role: &str,
        revision: i64,
        data: &StoredData,
        loro_update: Option<&[u8]>,
    ) -> StorageResult<()> {
        if role == "editor" {
            let update = loro_update
                .filter(|bytes| !bytes.is_empty())
                .ok_or("Editable snapshot is missing its Loro document")?;
            let projected = crate::loro_board::project(update)
                .map_err(|error| format!("Invalid Loro document: {error}"))?;
            if projected.columns != data.columns
                || projected.archives != data.archives
                || projected.templates != data.templates
            {
                return Err("Shared snapshot and Loro document disagree".into());
            }
        }
        let tx = self.connection.transaction()?;
        if tx
            .query_row("SELECT 1 FROM cardbe_boards WHERE id=?1", [board_id], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .is_none()
        {
            return Err("Board not found".into());
        }
        write_board_transaction(&tx, board_id, data)?;
        if let Some(update) = loro_update.filter(|bytes| !bytes.is_empty()) {
            tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, update])?;
        } else {
            tx.execute("DELETE FROM cardbe_loro_docs WHERE board_id=?1", [board_id])?;
        }
        tx.execute("UPDATE cardbe_boards SET name=?2,shared_role=?3,sync_status='synced',sync_revision=?4 WHERE id=?1", params![board_id, name, role, revision])?;
        tx.execute(
            "DELETE FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id],
        )?;
        tx.commit()?;
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(())
    }

    pub fn use_iroh_conflict_remote(
        &mut self,
        board_id: i64,
        local_name: &str,
        local: &StoredData,
        remote_name: &str,
        role: &str,
        revision: i64,
        remote: &StoredData,
    ) -> StorageResult<Board> {
        let tx = self.connection.transaction()?;
        let copy_name = format!("{local_name} (conflict copy)");
        tx.execute("INSERT INTO cardbe_boards(name) VALUES(?1)", [&copy_name])?;
        let copy_id = tx.last_insert_rowid();
        write_board_transaction(&tx, copy_id, local)?;
        write_board_transaction(&tx, board_id, remote)?;
        tx.execute("UPDATE cardbe_boards SET name=?2,shared_role=?3,sync_status='synced',sync_revision=?4 WHERE id=?1", params![board_id, remote_name, role, revision])?;
        tx.execute(
            "DELETE FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id],
        )?;
        tx.commit()?;
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(Board {
            id: copy_id,
            name: copy_name,
            task_count: local
                .columns
                .iter()
                .map(|column| column.tasks.len() as i64)
                .sum(),
            shared_role: "owner".into(),
            sync_status: "local".into(),
            sync_revision: 0,
        })
    }

    pub fn replace_board_as_local_edit(
        &mut self,
        board_id: i64,
        data: &StoredData,
    ) -> StorageResult<(i64, String)> {
        // The Loro delta must compare with the persisted board, not an empty
        // value: otherwise removals are absent from the CRDT and reappear on
        // the next editor sync.
        let before = self.read_board_complete(board_id)?;
        let tx = self.connection.transaction()?;
        let role: String = tx.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [board_id],
            |r| r.get(0),
        )?;
        if role == "viewer" {
            return Err("This shared board is read-only".into());
        }
        write_board_transaction(&tx, board_id, data)?;
        mark_local_board_change(&tx, board_id, &role)?;
        persist_loro_local_delta(&tx, board_id, &before, data)?;
        tx.commit()?;
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        self.board_sync_state(board_id)
    }

    /// Create and populate a board atomically.
    pub fn create_board_with_data(
        &mut self,
        name: &str,
        data: &StoredData,
    ) -> StorageResult<Board> {
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO cardbe_boards(name) VALUES (?1)", [name])?;
        let id = tx.last_insert_rowid();
        write_board_transaction(&tx, id, data)?;
        persist_loro_local_delta(&tx, id, &StoredData::default(), data)?;
        tx.commit()?;
        Ok(Board {
            id,
            name: name.into(),
            task_count: data
                .columns
                .iter()
                .map(|column| column.tasks.len() as i64)
                .sum(),
            shared_role: "owner".into(),
            sync_status: "local".into(),
            sync_revision: 0,
        })
    }

    pub fn rename_board(&mut self, id: i64, name: &str) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        let role: String = tx.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if role != "owner" {
            return Err("Only the owner can rename a shared board".into());
        }
        tx.execute(
            "UPDATE cardbe_boards SET name=?2 WHERE id=?1",
            params![id, name],
        )?;
        mark_local_board_change(&tx, id, &role)?;
        tx.commit()?;
        Ok(())
    }

    pub fn create_received_board(
        &mut self,
        name: &str,
        data: &StoredData,
        role: &str,
        revision: i64,
        ticket: &str,
        loro_update: Option<&[u8]>,
    ) -> StorageResult<Board> {
        if role == "editor" {
            let update = loro_update
                .filter(|bytes| !bytes.is_empty())
                .ok_or("Editable invitation is missing its Loro document")?;
            let projected = crate::loro_board::project(update)
                .map_err(|error| format!("Invalid Loro document: {error}"))?;
            if projected.columns != data.columns
                || projected.archives != data.archives
                || projected.templates != data.templates
            {
                return Err("Shared snapshot and Loro document disagree".into());
            }
        }
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO cardbe_boards(name,shared_role,sync_status,sync_revision) VALUES(?1,?2,'synced',?3)", params![name, role, revision])?;
        let id = tx.last_insert_rowid();
        write_board_transaction(&tx, id, data)?;
        if let Some(update) = loro_update.filter(|update| !update.is_empty()) {
            tx.execute(
                "INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2)",
                params![id, update],
            )?;
        }
        tx.execute(
            "INSERT INTO cardbe_iroh_remotes(board_id,ticket) VALUES(?1,?2)",
            params![id, ticket],
        )?;
        tx.commit()?;
        Ok(Board {
            id,
            name: name.into(),
            task_count: data
                .columns
                .iter()
                .map(|column| column.tasks.len() as i64)
                .sum(),
            shared_role: role.into(),
            sync_status: "synced".into(),
            sync_revision: revision,
        })
    }

    pub fn delete_board(&mut self, id: i64) -> StorageResult<()> {
        if self
            .connection
            .execute("DELETE FROM cardbe_boards WHERE id = ?1", [id])?
            == 0
        {
            return Err("Board not found".into());
        }
        Ok(())
    }

    fn set_active_board_id(&mut self, id: i64) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO cardbe_global_metadata(key, value) VALUES ('active_board_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [id.to_string()],
        )?;
        self.active_board_id = id;
        Ok(())
    }

    pub fn create_note_with_content(
        &mut self,
        now: i64,
        title: String,
        content: String,
    ) -> StorageResult<Note> {
        self.connection.execute(
            "INSERT INTO notes(title, content, pinned, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?3)",
            params![title, content, now],
        )?;
        Ok(Note {
            id: self.connection.last_insert_rowid(),
            title,
            content,
            pinned: false,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update_note(&mut self, note: &Note) -> StorageResult<()> {
        let changed = self.connection.execute(
            "UPDATE notes SET title = ?2, content = ?3, pinned = ?4,
                 updated_at = ?5 WHERE id = ?1",
            params![
                note.id,
                note.title,
                note.content,
                note.pinned,
                note.updated_at
            ],
        )?;
        if changed == 0 {
            return Err(format!("Note not found: {}", note.id).into());
        }
        Ok(())
    }

    pub fn delete_note(&mut self, note_id: i64) -> StorageResult<()> {
        let changed = self
            .connection
            .execute("DELETE FROM notes WHERE id = ?1", [note_id])?;
        if changed == 0 {
            return Err(format!("Note not found: {note_id}").into());
        }
        Ok(())
    }

    pub fn persist_diff(
        &mut self,
        before: &StoredData,
        after: &StoredData,
    ) -> StorageResult<PersistStats> {
        self.persist_diff_internal(before, after, true)
    }

    pub fn persist_diff_without_archives(
        &mut self,
        before: &StoredData,
        after: &StoredData,
    ) -> StorageResult<PersistStats> {
        self.persist_diff_internal(before, after, false)
    }

    fn persist_diff_internal(
        &mut self,
        before: &StoredData,
        after: &StoredData,
        include_archives: bool,
    ) -> StorageResult<PersistStats> {
        let mut positions = self.positions.clone();
        let transaction = self.connection.transaction()?;
        if self.active_board_id == 0 {
            return Err("No active board is selected".into());
        }
        let board_changed = before.columns != after.columns
            || before.archives != after.archives
            || before.templates != after.templates
            || before.label_recency != after.label_recency
            || before.next_column_id != after.next_column_id
            || before.next_task_id != after.next_task_id
            || before.next_template_id != after.next_template_id
            || before.schema_version != after.schema_version;
        let role: String = transaction.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [self.active_board_id],
            |r| r.get(0),
        )?;
        if board_changed && role == "viewer" {
            return Err("This shared board is read-only".into());
        }
        let stats = persist_board_diff_transaction(
            &transaction,
            self.active_board_id,
            before,
            after,
            &mut positions,
            include_archives,
        )?;
        // Settings are global application preferences, not board content.
        transaction.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES ('settings',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [serde_json::to_string(&after.settings)?],
        )?;
        if board_changed {
            mark_local_board_change(&transaction, self.active_board_id, &role)?;
            if matches!(role.as_str(), "owner" | "editor") {
                persist_loro_local_delta(&transaction, self.active_board_id, before, after)?;
            }
        }
        transaction.commit()?;
        self.positions = positions;
        Ok(stats)
    }

    fn replace_all(&mut self, data: &StoredData) -> StorageResult<()> {
        let mut positions = DatabasePositions::default();
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(
            "DELETE FROM tasks;
             DELETE FROM columns;
             DELETE FROM archives;
             DELETE FROM templates;
             DELETE FROM metadata;",
        )?;
        persist_diff_transaction(
            &transaction,
            &StoredData::default(),
            data,
            &mut positions,
            true,
        )?;
        transaction.execute(
            "INSERT INTO metadata(key, value) VALUES
                ('storage_initialized', '1'),
                ('lazy_archives_ready', '1')",
            [],
        )?;
        transaction.commit()?;
        self.positions = positions;
        if self.active_board_id != 0 {
            let board_id = self.active_board_id;
            let tx = self.connection.transaction()?;
            tx.execute("DELETE FROM cardbe_columns WHERE board_id=?1", [board_id])?;
            tx.execute("DELETE FROM cardbe_archives WHERE board_id=?1", [board_id])?;
            tx.execute("DELETE FROM cardbe_templates WHERE board_id=?1", [board_id])?;
            tx.execute(
                "DELETE FROM cardbe_board_metadata WHERE board_id=?1",
                [board_id],
            )?;
            write_board_transaction(&tx, board_id, data)?;
            tx.commit()?;
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(())
    }

    fn is_initialized(&self) -> StorageResult<bool> {
        Ok(metadata_value(&self.connection, "storage_initialized")?.as_deref() == Some("1"))
    }

    fn load(&self) -> StorageResult<StoredData> {
        if self.active_board_id != 0 {
            return self.load_board_data(self.active_board_id, true);
        }
        self.load_internal(true)
    }

    fn load_internal(&self, include_archives: bool) -> StorageResult<StoredData> {
        let mut stored = StoredData::default();

        let mut columns = self
            .connection
            .prepare("SELECT id, name, color, sort_order FROM columns ORDER BY position, id")?;
        stored.columns = columns
            .query_map([], |row| {
                Ok(Column {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    sort_order: ColumnSort::from_str(&row.get::<_, String>(3)?),
                    tasks: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let column_indices = stored
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| (column.id, index))
            .collect::<HashMap<_, _>>();
        let mut tasks = self
            .connection
            .prepare("SELECT column_id, payload FROM tasks ORDER BY column_id, position, id")?;
        let task_rows = tasks
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (column_id, payload) in task_rows {
            let task: Task = serde_json::from_str(&payload)?;
            let index = column_indices
                .get(&column_id)
                .ok_or_else(|| format!("Task {} refers to missing column {column_id}", task.id))?;
            stored.columns[*index].tasks.push(task);
        }

        if include_archives {
            stored.archives = self.load_archives()?;
        }

        let mut templates = self
            .connection
            .prepare("SELECT id, name, payload FROM templates ORDER BY position, id")?;
        let template_rows = templates
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        stored.templates = template_rows
            .into_iter()
            .map(|(id, name, payload)| {
                Ok(TaskTemplate {
                    id,
                    name,
                    task: serde_json::from_str(&payload)?,
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;

        stored.schema_version =
            metadata_parse(&self.connection, "schema_version")?.unwrap_or(stored.schema_version);
        stored.next_column_id =
            metadata_parse(&self.connection, "next_column_id")?.unwrap_or_default();
        stored.next_task_id = metadata_parse(&self.connection, "next_task_id")?.unwrap_or_default();
        stored.next_template_id =
            metadata_parse(&self.connection, "next_template_id")?.unwrap_or_default();
        stored.settings = metadata_json(&self.connection, "settings")?.unwrap_or_default();
        stored.label_recency =
            metadata_json(&self.connection, "label_recency")?.unwrap_or_default();
        Ok(stored)
    }

    pub fn load_archives(&self) -> StorageResult<Vec<Archive>> {
        if self.active_board_id != 0 {
            return self.load_board_archives(self.active_board_id);
        }
        let mut archives = self
            .connection
            .prepare("SELECT archived_at, payload FROM archives ORDER BY position, task_id")?;
        let archive_rows = archives
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        archive_rows
            .into_iter()
            .map(|(time, payload)| {
                Ok(Archive {
                    time: time.parse()?,
                    task: serde_json::from_str(&payload)?,
                })
            })
            .collect()
    }

    fn lazy_archives_ready(&self) -> StorageResult<bool> {
        Ok(metadata_value(&self.connection, "lazy_archives_ready")?.as_deref() == Some("1"))
    }

    fn mark_lazy_archives_ready(&self) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO metadata(key, value) VALUES ('lazy_archives_ready', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )?;
        Ok(())
    }

    fn migrate_multiboard_schema(&mut self, legacy: &StoredData) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS cardbe_global_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS cardbe_boards(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL,shared_role TEXT NOT NULL DEFAULT 'owner',sync_status TEXT NOT NULL DEFAULT 'local',sync_revision INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS cardbe_columns(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,id INTEGER NOT NULL,name TEXT NOT NULL,color TEXT NOT NULL,sort_order TEXT NOT NULL,position INTEGER NOT NULL,PRIMARY KEY(board_id,id));
             CREATE TABLE IF NOT EXISTS cardbe_tasks(board_id INTEGER NOT NULL,id INTEGER NOT NULL,column_id INTEGER NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,id),FOREIGN KEY(board_id,column_id) REFERENCES cardbe_columns(board_id,id) ON DELETE CASCADE);
             CREATE INDEX IF NOT EXISTS cardbe_tasks_by_board_column_position ON cardbe_tasks(board_id,column_id,position);
             CREATE TABLE IF NOT EXISTS cardbe_archives(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,task_id INTEGER NOT NULL,archived_at TEXT NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,task_id));
             CREATE TABLE IF NOT EXISTS cardbe_templates(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,id INTEGER NOT NULL,name TEXT NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,id));
             CREATE TABLE IF NOT EXISTS cardbe_board_metadata(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,key TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(board_id,key));"
        )?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS cardbe_iroh_invites(invite_id TEXT PRIMARY KEY,board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,secret TEXT NOT NULL,permission TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1,created_at TEXT NOT NULL DEFAULT (datetime('now')));
            CREATE TABLE IF NOT EXISTS cardbe_iroh_remotes(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,ticket TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cardbe_iroh_conflicts(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,revision INTEGER NOT NULL,name TEXT NOT NULL,permission TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cardbe_loro_docs(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,payload BLOB NOT NULL);")?;
        for sql in [
            "ALTER TABLE cardbe_boards ADD COLUMN shared_role TEXT NOT NULL DEFAULT 'owner'",
            "ALTER TABLE cardbe_boards ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'local'",
            "ALTER TABLE cardbe_boards ADD COLUMN sync_revision INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE cardbe_iroh_invites ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE cardbe_iroh_invites ADD COLUMN created_at TEXT NOT NULL DEFAULT ''",
        ] {
            let _ = tx.execute(sql, []);
        }
        validate_cardbe_schema(&tx)?;
        let migrated: Option<String> = tx
            .query_row(
                "SELECT value FROM cardbe_global_metadata WHERE key='multiboard_schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if migrated.as_deref() != Some("1") {
            let experimental = experimental_snapshots(&tx)?;
            if experimental.is_empty() {
                tx.execute("INSERT INTO cardbe_boards(name) VALUES ('My board')", [])?;
                let id = tx.last_insert_rowid();
                write_board_transaction(&tx, id, legacy)?;
                tx.execute(
                    "INSERT INTO cardbe_global_metadata(key,value) VALUES ('active_board_id',?1)",
                    [id.to_string()],
                )?;
            } else {
                let mut first = None;
                for (old_id, name, data) in experimental {
                    tx.execute(
                        "INSERT INTO cardbe_boards(id,name) VALUES (?1,?2)",
                        params![old_id, name],
                    )?;
                    write_board_transaction(&tx, old_id, &data)?;
                    first.get_or_insert(old_id);
                }
                let preferred = legacy_metadata_parse::<i64>(&tx, "active_board_id")?
                    .filter(|id| {
                        tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM cardbe_boards WHERE id=?1)",
                            [id],
                            |r| r.get::<_, bool>(0),
                        )
                        .unwrap_or(false)
                    })
                    .or(first)
                    .ok_or("Experimental board migration contained no boards")?;
                tx.execute(
                    "INSERT INTO cardbe_global_metadata(key,value) VALUES ('active_board_id',?1)",
                    [preferred.to_string()],
                )?;
            }
            tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES ('settings',?1) ON CONFLICT(key) DO NOTHING", [serde_json::to_string(&legacy.settings)?])?;
            tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES ('multiboard_schema_version','1')", [])?;
        }
        tx.commit()?;
        Ok(())
    }

    fn write_board(&mut self, board_id: i64, data: &StoredData) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        write_board_transaction(&tx, board_id, data)?;
        tx.commit()?;
        Ok(())
    }

    fn load_board_positions(&self, board_id: i64) -> StorageResult<DatabasePositions> {
        Ok(DatabasePositions {
            columns: load_board_positions(&self.connection, "cardbe_columns", board_id)?,
            tasks: load_board_positions(&self.connection, "cardbe_tasks", board_id)?,
        })
    }

    fn load_board_data(&self, board_id: i64, include_archives: bool) -> StorageResult<StoredData> {
        let mut stored = StoredData::default();
        let mut stmt=self.connection.prepare("SELECT id,name,color,sort_order FROM cardbe_columns WHERE board_id=?1 ORDER BY position,id")?;
        stored.columns = stmt
            .query_map([board_id], |r| {
                Ok(Column {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    color: r.get(2)?,
                    sort_order: ColumnSort::from_str(&r.get::<_, String>(3)?),
                    tasks: vec![],
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let indexes = stored
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect::<HashMap<_, _>>();
        let mut stmt=self.connection.prepare("SELECT column_id,payload FROM cardbe_tasks WHERE board_id=?1 ORDER BY column_id,position,id")?;
        for row in stmt.query_map([board_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })? {
            let (column_id, payload) = row?;
            let task: Task = serde_json::from_str(&payload)?;
            let index = indexes.get(&column_id).ok_or_else(|| {
                format!(
                    "Board {board_id} task {} refers to missing column {column_id}",
                    task.id
                )
            })?;
            stored.columns[*index].tasks.push(task);
        }
        if include_archives {
            stored.archives = self.load_board_archives(board_id)?;
        }
        let mut stmt = self.connection.prepare(
            "SELECT id,name,payload FROM cardbe_templates WHERE board_id=?1 ORDER BY position,id",
        )?;
        stored.templates = stmt
            .query_map([board_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .map(|row| {
                let (id, name, payload) = row?;
                Ok(TaskTemplate {
                    id,
                    name,
                    task: serde_json::from_str(&payload).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            })
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        stored.schema_version = board_metadata_parse(&self.connection, board_id, "schema_version")?
            .unwrap_or(CURRENT_SCHEMA_VERSION);
        stored.next_column_id =
            board_metadata_parse(&self.connection, board_id, "next_column_id")?.unwrap_or_default();
        stored.next_task_id =
            board_metadata_parse(&self.connection, board_id, "next_task_id")?.unwrap_or_default();
        stored.next_template_id =
            board_metadata_parse(&self.connection, board_id, "next_template_id")?
                .unwrap_or_default();
        stored.label_recency =
            board_metadata_json(&self.connection, board_id, "label_recency")?.unwrap_or_default();
        stored.settings =
            namespaced_metadata_json(&self.connection, "settings")?.unwrap_or_default();
        Ok(stored)
    }

    fn load_board_archives(&self, board_id: i64) -> StorageResult<Vec<Archive>> {
        let mut stmt=self.connection.prepare("SELECT archived_at,payload FROM cardbe_archives WHERE board_id=?1 ORDER BY position,task_id")?;
        let result = stmt
            .query_map([board_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .map(|row| {
                let (time, payload) = row?;
                Ok(Archive {
                    time: time.parse()?,
                    task: serde_json::from_str(&payload)?,
                })
            })
            .collect();
        result
    }
}

pub fn load(app_data_dir: &Path) -> StorageResult<LoadedData> {
    fs::create_dir_all(app_data_dir)?;
    let database_path = app_data_dir.join("data.sqlite3");
    let database_existed = database_path.try_exists()?;
    let mut messages = Vec::new();
    let mut database = Database::open(database_path)?;

    let (legacy, _archives_loaded) = if database_existed && database.is_initialized()? {
        let archives_loaded = !database.lazy_archives_ready()?;
        // Snapshots must include archives even when the legacy store is still
        // using lazy archive hydration, otherwise the first board migration
        // would omit them.
        let stored = database.load()?;
        let (migrated, changed) = stored.clone().migrate().map_err(std::io::Error::other)?;
        if changed {
            if archives_loaded {
                database.persist_diff(&stored, &migrated)?;
            } else {
                database.persist_diff_without_archives(&stored, &migrated)?;
            }
        }
        if archives_loaded {
            database.mark_lazy_archives_ready()?;
        }
        (migrated, archives_loaded)
    } else {
        let (stored, imported_current_json) = load_json_or_legacy(app_data_dir, &mut messages)?;
        let (stored, _) = stored.migrate().map_err(std::io::Error::other)?;
        database.replace_all(&stored).map_err(|error| {
            std::io::Error::other(format!(
                "Could not migrate application data to SQLite; the source files were left unchanged: {error}"
            ))
        })?;
        if imported_current_json {
            preserve_migrated_json(app_data_dir, &mut messages);
        }
        (stored, true)
    };

    // Board snapshots were introduced after the original single-board SQLite
    // layout.  The first run copies that complete document into the default
    // board before any future writes, preserving every old field verbatim.
    let (_boards, stored) = database.initialize_boards(&legacy)?;
    let (mut stored, changed) = stored.migrate().map_err(std::io::Error::other)?;
    if changed {
        database.persist_diff(&legacy, &stored)?;
    }
    if !_archives_loaded {
        stored.archives.clear();
    }
    Ok(LoadedData {
        stored,
        database,
        recovery_messages: messages,
        archives_loaded: _archives_loaded,
    })
}

fn persist_diff_transaction(
    transaction: &Transaction<'_>,
    before: &StoredData,
    after: &StoredData,
    positions: &mut DatabasePositions,
    include_archives: bool,
) -> StorageResult<PersistStats> {
    let mut stats = PersistStats::default();
    let before_tasks = task_locations(before);
    let after_tasks = task_locations(after);

    let column_ids = after
        .columns
        .iter()
        .map(|column| column.id)
        .collect::<Vec<_>>();
    let column_positions = assign_sparse_positions(&column_ids, &positions.columns)?;

    let mut task_positions = HashMap::new();
    for column in &after.columns {
        let ids = column.tasks.iter().map(|task| task.id).collect::<Vec<_>>();
        let old_positions = before
            .columns
            .iter()
            .find(|old_column| old_column.id == column.id)
            .into_iter()
            .flat_map(|old_column| old_column.tasks.iter())
            .filter_map(|task| {
                positions
                    .tasks
                    .get(&task.id)
                    .copied()
                    .map(|position| (task.id, position))
            })
            .collect::<HashMap<_, _>>();
        task_positions.extend(assign_sparse_positions(&ids, &old_positions)?);
    }

    for id in before_tasks
        .keys()
        .filter(|id| !after_tasks.contains_key(id))
    {
        transaction.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        stats.tasks_changed += 1;
    }

    let before_columns = before
        .columns
        .iter()
        .map(|column| (column.id, column))
        .collect::<HashMap<_, _>>();
    let after_columns = after
        .columns
        .iter()
        .map(|column| (column.id, column))
        .collect::<HashMap<_, _>>();
    for id in before_columns
        .keys()
        .filter(|id| !after_columns.contains_key(id))
    {
        transaction.execute("DELETE FROM columns WHERE id = ?1", [id])?;
        stats.columns_changed += 1;
    }
    for (id, column) in &after_columns {
        let position = column_positions[id];
        let changed = before_columns.get(id).is_none_or(|old| {
            positions.columns.get(id) != Some(&position)
                || old.name != column.name
                || old.color != column.color
                || old.sort_order != column.sort_order
        });
        if changed {
            transaction.execute(
                "INSERT INTO columns(id, name, color, sort_order, position) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, color = excluded.color,
                   sort_order = excluded.sort_order, position = excluded.position",
                params![id, column.name, column.color, column.sort_order.as_str(), position],
            )?;
            stats.columns_changed += 1;
        }
    }

    for (id, (column_id, task)) in &after_tasks {
        let position = task_positions[id];
        let changed = before_tasks.get(id).is_none_or(|(old_column, old)| {
            old_column != column_id || positions.tasks.get(id) != Some(&position) || old != task
        });
        if changed {
            transaction.execute(
                "INSERT INTO tasks(id, column_id, position, payload) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                   column_id = excluded.column_id,
                   position = excluded.position,
                   payload = excluded.payload",
                params![id, column_id, position, serde_json::to_string(task)?],
            )?;
            stats.tasks_changed += 1;
        }
    }

    if include_archives {
        stats.archives_changed = sync_archives(transaction, &before.archives, &after.archives)?;
    }
    stats.templates_changed = sync_templates(transaction, &before.templates, &after.templates)?;

    let metadata = [
        (
            "schema_version",
            before.schema_version.to_string(),
            after.schema_version.to_string(),
        ),
        (
            "next_column_id",
            before.next_column_id.to_string(),
            after.next_column_id.to_string(),
        ),
        (
            "next_task_id",
            before.next_task_id.to_string(),
            after.next_task_id.to_string(),
        ),
        (
            "next_template_id",
            before.next_template_id.to_string(),
            after.next_template_id.to_string(),
        ),
        (
            "settings",
            serde_json::to_string(&before.settings)?,
            serde_json::to_string(&after.settings)?,
        ),
        (
            "label_recency",
            serde_json::to_string(&before.label_recency)?,
            serde_json::to_string(&after.label_recency)?,
        ),
    ];
    for (key, old, new) in metadata {
        if old != new || !metadata_exists(transaction, key)? {
            transaction.execute(
                "INSERT INTO metadata(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, new],
            )?;
            stats.metadata_changed += 1;
        }
    }
    positions.columns = column_positions;
    positions.tasks = task_positions;
    Ok(stats)
}

fn persist_board_diff_transaction(
    tx: &Transaction<'_>,
    board_id: i64,
    before: &StoredData,
    after: &StoredData,
    positions: &mut DatabasePositions,
    include_archives: bool,
) -> StorageResult<PersistStats> {
    let mut stats = PersistStats::default();
    let before_tasks = task_locations(before);
    let after_tasks = task_locations(after);
    let column_ids = after.columns.iter().map(|c| c.id).collect::<Vec<_>>();
    let column_positions = assign_sparse_positions(&column_ids, &positions.columns)?;
    let mut task_positions = HashMap::new();
    for column in &after.columns {
        let ids = column.tasks.iter().map(|t| t.id).collect::<Vec<_>>();
        let old = column
            .tasks
            .iter()
            .filter_map(|t| positions.tasks.get(&t.id).map(|p| (t.id, *p)))
            .collect();
        task_positions.extend(assign_sparse_positions(&ids, &old)?);
    }
    for id in before_tasks
        .keys()
        .filter(|id| !after_tasks.contains_key(id))
    {
        tx.execute(
            "DELETE FROM cardbe_tasks WHERE board_id=?1 AND id=?2",
            params![board_id, id],
        )?;
        stats.tasks_changed += 1;
    }
    let bc = before
        .columns
        .iter()
        .map(|c| (c.id, c))
        .collect::<HashMap<_, _>>();
    let ac = after
        .columns
        .iter()
        .map(|c| (c.id, c))
        .collect::<HashMap<_, _>>();
    for id in bc.keys().filter(|id| !ac.contains_key(id)) {
        tx.execute(
            "DELETE FROM cardbe_columns WHERE board_id=?1 AND id=?2",
            params![board_id, id],
        )?;
        stats.columns_changed += 1;
    }
    for (id, c) in &ac {
        let pos = column_positions[id];
        if bc.get(id).is_none_or(|o| {
            positions.columns.get(id) != Some(&pos)
                || o.name != c.name
                || o.color != c.color
                || o.sort_order != c.sort_order
        }) {
            tx.execute("INSERT INTO cardbe_columns(board_id,id,name,color,sort_order,position) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(board_id,id) DO UPDATE SET name=excluded.name,color=excluded.color,sort_order=excluded.sort_order,position=excluded.position",params![board_id,id,c.name,c.color,c.sort_order.as_str(),pos])?;
            stats.columns_changed += 1;
        }
    }
    for (id, (column_id, task)) in &after_tasks {
        let pos = task_positions[id];
        if before_tasks.get(id).is_none_or(|(oc, ot)| {
            oc != column_id || positions.tasks.get(id) != Some(&pos) || ot != task
        }) {
            tx.execute("INSERT INTO cardbe_tasks(board_id,id,column_id,position,payload) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(board_id,id) DO UPDATE SET column_id=excluded.column_id,position=excluded.position,payload=excluded.payload",params![board_id,id,column_id,pos,serde_json::to_string(task)?])?;
            stats.tasks_changed += 1;
        }
    }
    if include_archives && before.archives != after.archives {
        tx.execute("DELETE FROM cardbe_archives WHERE board_id=?1", [board_id])?;
        for (i, a) in after.archives.iter().enumerate() {
            tx.execute("INSERT INTO cardbe_archives(board_id,task_id,archived_at,position,payload) VALUES(?1,?2,?3,?4,?5)",params![board_id,a.task.id,a.time.to_string(),i as i64,serde_json::to_string(&a.task)?])?;
        }
        stats.archives_changed = after.archives.len().max(before.archives.len());
    }
    if before.templates != after.templates {
        tx.execute("DELETE FROM cardbe_templates WHERE board_id=?1", [board_id])?;
        for (i, t) in after.templates.iter().enumerate() {
            tx.execute("INSERT INTO cardbe_templates(board_id,id,name,position,payload) VALUES(?1,?2,?3,?4,?5)",params![board_id,t.id,t.name,i as i64,serde_json::to_string(&t.task)?])?;
        }
        stats.templates_changed = after.templates.len().max(before.templates.len());
    }
    let metadata = [
        ("schema_version", after.schema_version.to_string()),
        ("next_column_id", after.next_column_id.to_string()),
        ("next_task_id", after.next_task_id.to_string()),
        ("next_template_id", after.next_template_id.to_string()),
        (
            "label_recency",
            serde_json::to_string(&after.label_recency)?,
        ),
    ];
    for (key, value) in metadata {
        let current: Option<String> = tx
            .query_row(
                "SELECT value FROM cardbe_board_metadata WHERE board_id=?1 AND key=?2",
                params![board_id, key],
                |r| r.get(0),
            )
            .optional()?;
        if current.as_deref() != Some(value.as_str()) {
            tx.execute("INSERT INTO cardbe_board_metadata(board_id,key,value) VALUES(?1,?2,?3) ON CONFLICT(board_id,key) DO UPDATE SET value=excluded.value",params![board_id,key,value])?;
            stats.metadata_changed += 1;
        }
    }
    positions.columns = column_positions;
    positions.tasks = task_positions;
    Ok(stats)
}

fn write_board_transaction(
    tx: &Transaction<'_>,
    board_id: i64,
    data: &StoredData,
) -> StorageResult<()> {
    // A snapshot is authoritative. Diffing against an empty value alone cannot
    // remove rows that are absent from the incoming snapshot.
    tx.execute("DELETE FROM cardbe_tasks WHERE board_id=?1", [board_id])?;
    tx.execute("DELETE FROM cardbe_columns WHERE board_id=?1", [board_id])?;
    tx.execute("DELETE FROM cardbe_archives WHERE board_id=?1", [board_id])?;
    tx.execute("DELETE FROM cardbe_templates WHERE board_id=?1", [board_id])?;
    tx.execute(
        "DELETE FROM cardbe_board_metadata WHERE board_id=?1",
        [board_id],
    )?;
    let mut p = DatabasePositions::default();
    persist_board_diff_transaction(tx, board_id, &StoredData::default(), data, &mut p, true)?;
    Ok(())
}

fn mark_local_board_change(tx: &Transaction<'_>, board_id: i64, role: &str) -> StorageResult<()> {
    match role {
        "owner" => {
            tx.execute(
                "UPDATE cardbe_boards SET sync_revision=sync_revision+1 WHERE id=?1",
                [board_id],
            )?;
        }
        "editor" => {
            tx.execute(
                "UPDATE cardbe_boards SET sync_status=CASE WHEN sync_status='conflict' THEN 'conflict' ELSE 'pending' END WHERE id=?1",
                [board_id],
            )?;
        }
        _ => return Err("This shared board is read-only".into()),
    }
    Ok(())
}

fn persist_loro_local_delta(
    tx: &Transaction<'_>,
    board_id: i64,
    before: &StoredData,
    after: &StoredData,
) -> StorageResult<()> {
    let existing = tx
        .query_row(
            "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
            [board_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    // Legacy boards have no document yet. Seed it from the complete current
    // projection, then subsequent writes are deltas.
    let update = if existing.is_some() {
        crate::loro_board::apply_local_delta(existing.as_deref(), before, after)
    } else {
        crate::loro_board::apply_local_delta(None, &StoredData::default(), after)
    }
    .map_err(|error| format!("Could not update shared-board CRDT: {error}"))?;
    tx.execute(
        "INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2)
         ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload",
        params![board_id, update],
    )?;
    Ok(())
}

fn load_board_positions(
    connection: &Connection,
    table: &str,
    board_id: i64,
) -> StorageResult<HashMap<i64, i64>> {
    let mut s = connection.prepare(&format!(
        "SELECT id,position FROM {table} WHERE board_id=?1"
    ))?;
    let result = s
        .query_map([board_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(result)
}

fn table_columns(tx: &Transaction<'_>, table: &str) -> StorageResult<Vec<(String, String)>> {
    let mut s = tx.prepare(&format!("PRAGMA table_info({table})"))?;
    let result = s
        .query_map([], |r| Ok((r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    Ok(result)
}
fn table_exists(tx: &Transaction<'_>, table: &str) -> StorageResult<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}
fn validate_cardbe_schema(tx: &Transaction<'_>) -> StorageResult<()> {
    for (t, required) in [
        ("cardbe_boards", vec!["id", "name"]),
        (
            "cardbe_columns",
            vec!["board_id", "id", "name", "color", "sort_order", "position"],
        ),
        (
            "cardbe_tasks",
            vec!["board_id", "id", "column_id", "position", "payload"],
        ),
    ] {
        let columns = table_columns(tx, t)?
            .into_iter()
            .map(|v| v.0)
            .collect::<Vec<_>>();
        if !required.iter().all(|c| columns.iter().any(|v| v == c)) {
            return Err(format!(
                "Cardbe database schema conflict in {t}; expected columns: {}",
                required.join(", ")
            )
            .into());
        }
    }
    Ok(())
}

fn experimental_snapshots(tx: &Transaction<'_>) -> StorageResult<Vec<(i64, String, StoredData)>> {
    let has_boards = table_exists(tx, "boards")?;
    let has_snapshots = table_exists(tx, "board_snapshots")?;
    if !has_boards && !has_snapshots {
        return Ok(vec![]);
    }
    if has_boards && !has_snapshots {
        let bc = table_columns(tx, "boards")?;
        let looks_experimental = bc
            .iter()
            .any(|(n, t)| n == "id" && t.eq_ignore_ascii_case("INTEGER"))
            && bc.iter().any(|(n, _)| n == "name");
        if !looks_experimental {
            return Ok(vec![]);
        }
    }
    if !(has_boards && has_snapshots) {
        return Err("Incomplete experimental multi-board schema: both boards and board_snapshots are required. No data was changed.".into());
    }
    let bc = table_columns(tx, "boards")?;
    let sc = table_columns(tx, "board_snapshots")?;
    let valid = bc
        .iter()
        .any(|(n, t)| n == "id" && t.eq_ignore_ascii_case("INTEGER"))
        && bc.iter().any(|(n, _)| n == "name")
        && sc.iter().any(|(n, _)| n == "board_id")
        && sc.iter().any(|(n, _)| n == "payload");
    if !valid {
        return Ok(vec![]);
    } // Unknown/conflicting tables belong to another application.
    let mut s=tx.prepare("SELECT b.id,b.name,s.payload FROM boards b JOIN board_snapshots s ON s.board_id=b.id ORDER BY b.id")?;
    let rows = s
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let board_count: i64 = tx.query_row("SELECT count(*) FROM boards", [], |r| r.get(0))?;
    if rows.len() as i64 != board_count {
        return Err("Experimental board snapshots are incomplete; migration was rolled back to avoid data loss.".into());
    }
    rows.into_iter()
        .map(|(id, name, payload)| {
            let data = serde_json::from_str(&payload).map_err(|e| {
                format!("Invalid snapshot for experimental board {id} ({name}): {e}")
            })?;
            Ok((id, name, data))
        })
        .collect()
}

fn legacy_metadata_parse<T: std::str::FromStr>(
    tx: &Transaction<'_>,
    key: &str,
) -> StorageResult<Option<T>>
where
    T::Err: std::error::Error + 'static,
{
    let v: Option<String> = tx
        .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(v.map(|v| v.parse()).transpose()?)
}
fn namespaced_metadata_value(c: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    c.query_row(
        "SELECT value FROM cardbe_global_metadata WHERE key=?1",
        [key],
        |r| r.get(0),
    )
    .optional()
}
fn namespaced_metadata_parse<T: std::str::FromStr>(
    c: &Connection,
    key: &str,
) -> StorageResult<Option<T>>
where
    T::Err: std::error::Error + 'static,
{
    Ok(namespaced_metadata_value(c, key)?
        .map(|v| v.parse())
        .transpose()?)
}
fn namespaced_metadata_json<T: serde::de::DeserializeOwned>(
    c: &Connection,
    key: &str,
) -> StorageResult<Option<T>> {
    Ok(namespaced_metadata_value(c, key)?
        .map(|v| serde_json::from_str(&v))
        .transpose()?)
}
fn board_metadata_value(c: &Connection, b: i64, key: &str) -> rusqlite::Result<Option<String>> {
    c.query_row(
        "SELECT value FROM cardbe_board_metadata WHERE board_id=?1 AND key=?2",
        params![b, key],
        |r| r.get(0),
    )
    .optional()
}
fn board_metadata_parse<T: std::str::FromStr>(
    c: &Connection,
    b: i64,
    key: &str,
) -> StorageResult<Option<T>>
where
    T::Err: std::error::Error + 'static,
{
    Ok(board_metadata_value(c, b, key)?
        .map(|v| v.parse())
        .transpose()?)
}
fn board_metadata_json<T: serde::de::DeserializeOwned>(
    c: &Connection,
    b: i64,
    key: &str,
) -> StorageResult<Option<T>> {
    Ok(board_metadata_value(c, b, key)?
        .map(|v| serde_json::from_str(&v))
        .transpose()?)
}

fn task_locations(data: &StoredData) -> HashMap<i64, (i64, &Task)> {
    data.columns
        .iter()
        .flat_map(|column| {
            column
                .tasks
                .iter()
                .map(move |task| (task.id, (column.id, task)))
        })
        .collect()
}

fn assign_sparse_positions(
    ids: &[i64],
    old_positions: &HashMap<i64, i64>,
) -> StorageResult<HashMap<i64, i64>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let existing = ids
        .iter()
        .enumerate()
        .filter_map(|(index, id)| old_positions.get(id).map(|position| (index, *position)))
        .collect::<Vec<_>>();
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![None; existing.len()];
    for sequence_index in 0..existing.len() {
        let position = existing[sequence_index].1;
        let insertion = tails.partition_point(|tail| existing[*tail].1 < position);
        if insertion > 0 {
            previous[sequence_index] = Some(tails[insertion - 1]);
        }
        if insertion == tails.len() {
            tails.push(sequence_index);
        } else {
            tails[insertion] = sequence_index;
        }
    }

    let mut anchors = vec![false; ids.len()];
    let mut cursor = tails.last().copied();
    while let Some(sequence_index) = cursor {
        anchors[existing[sequence_index].0] = true;
        cursor = previous[sequence_index];
    }

    let mut assigned = HashMap::new();
    for (index, id) in ids.iter().enumerate() {
        if anchors[index] {
            assigned.insert(*id, old_positions[id]);
        }
    }

    let mut start = 0;
    while start < ids.len() {
        if anchors[start] {
            start += 1;
            continue;
        }
        let end = (start..ids.len())
            .find(|index| anchors[*index])
            .unwrap_or(ids.len());
        let count = end - start;
        let left = start
            .checked_sub(1)
            .filter(|index| anchors[*index])
            .map(|index| assigned[&ids[index]]);
        let right = (end < ids.len()).then(|| assigned[&ids[end]]);

        let values = sparse_values_between(left, right, count);
        let Some(values) = values else {
            return evenly_spaced_positions(ids);
        };
        for (id, position) in ids[start..end].iter().zip(values) {
            assigned.insert(*id, position);
        }
        start = end;
    }
    Ok(assigned)
}

fn sparse_values_between(left: Option<i64>, right: Option<i64>, count: usize) -> Option<Vec<i64>> {
    let count = i128::try_from(count).ok()?;
    match (left, right) {
        (Some(left), Some(right)) => {
            let available = i128::from(right) - i128::from(left);
            if available <= count {
                return None;
            }
            let step = available / (count + 1);
            (1..=count)
                .map(|offset| i64::try_from(i128::from(left) + step * offset).ok())
                .collect()
        }
        (Some(left), None) => (1..=count)
            .map(|offset| {
                i64::try_from(i128::from(left) + i128::from(SORT_POSITION_GAP) * offset).ok()
            })
            .collect(),
        (None, Some(right)) => (1..=count)
            .map(|offset| {
                let reverse_offset = count - offset + 1;
                i64::try_from(i128::from(right) - i128::from(SORT_POSITION_GAP) * reverse_offset)
                    .ok()
            })
            .collect(),
        (None, None) => evenly_spaced_values(count),
    }
}

fn evenly_spaced_values(count: i128) -> Option<Vec<i64>> {
    (1..=count)
        .map(|offset| i64::try_from(i128::from(SORT_POSITION_GAP) * offset).ok())
        .collect()
}

fn evenly_spaced_positions(ids: &[i64]) -> StorageResult<HashMap<i64, i64>> {
    let values = evenly_spaced_values(i128::try_from(ids.len())?)
        .ok_or("Too many items to assign sort positions")?;
    Ok(ids.iter().copied().zip(values).collect())
}

fn sync_archives(
    transaction: &Transaction<'_>,
    before: &[Archive],
    after: &[Archive],
) -> StorageResult<usize> {
    let old = before
        .iter()
        .enumerate()
        .map(|(position, archive)| (archive.task.id, (position, archive)))
        .collect::<HashMap<_, _>>();
    let new = after
        .iter()
        .enumerate()
        .map(|(position, archive)| (archive.task.id, (position, archive)))
        .collect::<HashMap<_, _>>();
    let mut changed = 0;
    for id in old.keys().filter(|id| !new.contains_key(id)) {
        transaction.execute("DELETE FROM archives WHERE task_id = ?1", [id])?;
        changed += 1;
    }
    for (id, (position, archive)) in new {
        if old
            .get(&id)
            .is_none_or(|(old_position, old)| old_position != &position || old != &archive)
        {
            transaction.execute(
                "INSERT INTO archives(task_id, archived_at, position, payload)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(task_id) DO UPDATE SET
                   archived_at = excluded.archived_at,
                   position = excluded.position,
                   payload = excluded.payload",
                params![
                    id,
                    archive.time.to_string(),
                    index_i64(position)?,
                    serde_json::to_string(&archive.task)?
                ],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

fn sync_templates(
    transaction: &Transaction<'_>,
    before: &[TaskTemplate],
    after: &[TaskTemplate],
) -> StorageResult<usize> {
    let old = before
        .iter()
        .enumerate()
        .map(|(position, template)| (template.id, (position, template)))
        .collect::<HashMap<_, _>>();
    let new = after
        .iter()
        .enumerate()
        .map(|(position, template)| (template.id, (position, template)))
        .collect::<HashMap<_, _>>();
    let mut changed = 0;
    for id in old.keys().filter(|id| !new.contains_key(id)) {
        transaction.execute("DELETE FROM templates WHERE id = ?1", [id])?;
        changed += 1;
    }
    for (id, (position, template)) in new {
        if old
            .get(&id)
            .is_none_or(|(old_position, old)| old_position != &position || old != &template)
        {
            transaction.execute(
                "INSERT INTO templates(id, name, position, payload) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   position = excluded.position,
                   payload = excluded.payload",
                params![
                    id,
                    template.name,
                    index_i64(position)?,
                    serde_json::to_string(&template.task)?
                ],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

fn metadata_exists(transaction: &Transaction<'_>, key: &str) -> rusqlite::Result<bool> {
    transaction
        .query_row("SELECT 1 FROM metadata WHERE key = ?1", [key], |_| Ok(()))
        .optional()
        .map(|value| value.is_some())
}

fn metadata_value(connection: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    connection
        .query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()
}

fn metadata_parse<T: std::str::FromStr>(
    connection: &Connection,
    key: &str,
) -> StorageResult<Option<T>>
where
    T::Err: std::error::Error + 'static,
{
    metadata_value(connection, key)?
        .map(|value| value.parse().map_err(Into::into))
        .transpose()
}

fn metadata_json<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    key: &str,
) -> StorageResult<Option<T>> {
    metadata_value(connection, key)?
        .map(|value| serde_json::from_str(&value).map_err(Into::into))
        .transpose()
}

fn index_i64(index: usize) -> StorageResult<i64> {
    Ok(i64::try_from(index)?)
}

fn load_json_or_legacy(
    app_data_dir: &Path,
    messages: &mut Vec<String>,
) -> StorageResult<(StoredData, bool)> {
    let data_path = app_data_dir.join("data.json");
    let backup = backup_path(&data_path);
    let had_current_files = data_path.try_exists()? || backup.try_exists()?;
    match read_document(&data_path) {
        Ok(Some(data)) => return Ok((data, true)),
        Ok(None) => {}
        Err(error) => messages.push(quarantine_file(&data_path, "application", &*error)),
    }
    match read_document(&backup) {
        Ok(Some(data)) => {
            messages.push(format!(
                "The SQLite database was migrated from {} because the primary JSON file was unavailable.",
                backup.display()
            ));
            return Ok((data, false));
        }
        Ok(None) => {}
        Err(error) => messages.push(quarantine_file(&backup, "backup", &*error)),
    }

    if had_current_files {
        messages.push(
            "No valid current application data could be recovered; a new empty database was created."
                .to_string(),
        );
        return Ok((StoredData::default(), false));
    }

    let stored = StoredData {
        columns: read_legacy(app_data_dir.join("tasks.json"), "legacy task", messages)
            .unwrap_or_default(),
        archives: read_legacy(
            app_data_dir.join("archives.json"),
            "legacy archive",
            messages,
        )
        .unwrap_or_default(),
        settings: read_legacy(
            app_data_dir.join("settings.json"),
            "legacy settings",
            messages,
        )
        .unwrap_or_default(),
        ..StoredData::default()
    };
    Ok((stored, false))
}

fn preserve_migrated_json(app_data_dir: &Path, messages: &mut Vec<String>) {
    let source = app_data_dir.join("data.json");
    if !source.exists() {
        return;
    }
    let destination = app_data_dir.join("data.migrated.json");
    if destination.exists() {
        return;
    }
    match fs::rename(&source, &destination) {
        Ok(()) => messages.push(format!(
            "The previous JSON data was migrated to SQLite and preserved at {}.",
            destination.display()
        )),
        Err(error) => messages.push(format!(
            "The data was migrated to SQLite, but the previous JSON file could not be renamed: {error}."
        )),
    }
}

fn read_document(path: &Path) -> StorageResult<Option<StoredData>> {
    if !path.try_exists()? {
        return Ok(None);
    }
    let file = File::open(path)?;
    Ok(Some(serde_json::from_reader(BufReader::new(file))?))
}

fn read_legacy<T: serde::de::DeserializeOwned>(
    path: PathBuf,
    name: &str,
    messages: &mut Vec<String>,
) -> Option<T> {
    if !path.exists() {
        return None;
    }
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) => {
            messages.push(quarantine_file(&path, name, &error));
            return None;
        }
    };
    match serde_json::from_reader(BufReader::new(file)) {
        Ok(data) => Some(data),
        Err(error) => {
            messages.push(quarantine_file(&path, name, &error));
            None
        }
    }
}

pub fn write_document(path: &Path, data: &StoredData) -> StorageResult<()> {
    let temp_path = path.with_extension("json.tmp");
    let backup = backup_path(path);
    let result = (|| {
        let mut file = File::create(&temp_path)?;
        serde_json::to_writer_pretty(&mut file, data)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        if path.try_exists()? {
            if backup.try_exists()? {
                fs::remove_file(&backup)?;
            }
            fs::rename(path, &backup)?;
        }
        if let Err(error) = fs::rename(&temp_path, path) {
            if !path.exists() && backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(error.into());
        }
        StorageResult::Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
}

fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.display()))
}

fn quarantine_file(path: &Path, name: &str, error: &dyn std::fmt::Display) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let quarantine = PathBuf::from(format!("{}.corrupt-{timestamp}", path.display()));
    match fs::rename(path, &quarantine) {
        Ok(()) => format!(
            "The {name} data file was unreadable and was moved to {} ({error}).",
            quarantine.display()
        ),
        Err(backup_error) => format!(
            "The {name} data file was unreadable ({error}), and could not be preserved: {backup_error}."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loro_push_is_atomic_idempotent_and_checks_current_permission() {
        let dir = test_dir("loro-capability");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let board_id = loaded.database.active_board_id();
        let mut base = StoredData::default();
        base.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![Task {
                id: 2,
                title: "Before".into(),
                ..Task::default()
            }],
        });
        loaded
            .database
            .replace_board_as_local_edit(board_id, &base)
            .unwrap();
        loaded
            .database
            .save_iroh_invite("editor", board_id, "secret", "editor")
            .unwrap();
        let original = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
        let mut edited = base.clone();
        edited.columns[0].tasks[0].title = "After".into();
        let update = crate::loro_board::apply_local_delta(Some(&original), &base, &edited).unwrap();
        let (changed, revision, data) = loaded
            .database
            .apply_iroh_loro_update(board_id, "editor", "secret", &update)
            .unwrap();
        assert!(changed);
        assert_eq!(data.columns[0].tasks[0].title, "After");
        assert_eq!(
            loaded
                .database
                .apply_iroh_loro_update(board_id, "editor", "secret", &update)
                .unwrap()
                .0,
            false
        );
        assert!(
            !loaded
                .database
                .apply_iroh_loro_update(board_id, "editor", "secret", &[])
                .unwrap()
                .0
        );
        assert_eq!(
            loaded.database.board_sync_state(board_id).unwrap().0,
            revision
        );
        loaded
            .database
            .update_iroh_invite("editor", None, Some(false))
            .unwrap();
        assert!(loaded
            .database
            .apply_iroh_loro_update(board_id, "editor", "secret", &update)
            .is_err());
        assert_eq!(
            loaded.database.board_sync_state(board_id).unwrap().0,
            revision
        );
        loaded
            .database
            .update_iroh_invite("editor", None, Some(true))
            .unwrap();
        loaded
            .database
            .connection
            .execute("DELETE FROM cardbe_loro_docs WHERE board_id=?1", [board_id])
            .unwrap();
        assert!(loaded
            .database
            .apply_iroh_loro_update(board_id, "editor", "secret", &[])
            .is_err());
        assert_eq!(
            loaded
                .database
                .read_board_complete(board_id)
                .unwrap()
                .columns[0]
                .tasks[0]
                .title,
            "After"
        );
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn offline_owner_and_editor_edits_converge_after_exchange() {
        let owner_dir = test_dir("loro-owner");
        let editor_dir = test_dir("loro-editor");
        fs::create_dir_all(&owner_dir).unwrap();
        fs::create_dir_all(&editor_dir).unwrap();
        let mut owner = load(&owner_dir).unwrap();
        let mut editor = load(&editor_dir).unwrap();
        let owner_id = owner.database.active_board_id();
        let mut base = StoredData::default();
        base.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![
                Task {
                    id: 2,
                    title: "A".into(),
                    ..Task::default()
                },
                Task {
                    id: 3,
                    title: "B".into(),
                    ..Task::default()
                },
            ],
        });
        owner
            .database
            .replace_board_as_local_edit(owner_id, &base)
            .unwrap();
        owner
            .database
            .save_iroh_invite("invite", owner_id, "secret", "editor")
            .unwrap();
        let initial = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
        let editor_id = editor
            .database
            .create_received_board("Shared", &base, "editor", 1, "ticket", Some(&initial))
            .unwrap()
            .id;
        let mut owner_edit = base.clone();
        owner_edit.columns[0].tasks[0].title = "Owner changed A".into();
        owner
            .database
            .replace_board_as_local_edit(owner_id, &owner_edit)
            .unwrap();
        let mut editor_edit = base.clone();
        editor_edit.columns[0].tasks[1].title = "Editor changed B".into();
        editor
            .database
            .replace_board_as_local_edit(editor_id, &editor_edit)
            .unwrap();
        let sent = editor
            .database
            .iroh_loro_update(editor_id)
            .unwrap()
            .unwrap();
        let vector = crate::loro_board::state_vector(Some(&sent)).unwrap();
        owner
            .database
            .apply_iroh_loro_update(owner_id, "invite", "secret", &sent)
            .unwrap();
        let owner_doc = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
        let diff = crate::loro_board::diff(Some(&owner_doc), &vector).unwrap();
        let revision = owner.database.board_sync_state(owner_id).unwrap().0;
        editor
            .database
            .merge_iroh_loro_diff(editor_id, &diff, "Shared", "editor", revision, &sent)
            .unwrap();
        let owner_data = owner.database.read_board_complete(owner_id).unwrap();
        let editor_data = editor.database.read_board_complete(editor_id).unwrap();
        assert_eq!(owner_data.columns, editor_data.columns);
        assert_eq!(owner_data.columns[0].tasks[0].title, "Owner changed A");
        assert_eq!(owner_data.columns[0].tasks[1].title, "Editor changed B");
        assert_eq!(
            editor.database.board_sync_state(editor_id).unwrap(),
            (revision, "synced".into())
        );
        drop(editor);
        let reopened = load(&editor_dir).unwrap();
        assert_eq!(
            reopened
                .database
                .read_board_complete(editor_id)
                .unwrap()
                .columns,
            owner_data.columns
        );
        assert!(reopened
            .database
            .iroh_loro_update(editor_id)
            .unwrap()
            .is_some());
        drop(reopened);
        drop(owner);
        fs::remove_dir_all(editor_dir).unwrap();
        fs::remove_dir_all(owner_dir).unwrap();
    }

    #[test]
    fn shared_snapshot_replaces_deleted_rows_and_preserves_global_settings() {
        let dir = test_dir("shared-snapshot-replace");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let board_id = loaded.database.active_board_id();
        let mut first = StoredData::default();
        first.columns.push(Column {
            id: 1,
            name: "Old".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![Task {
                id: 1,
                title: "Remove me".into(),
                ..Task::default()
            }],
        });
        first.next_column_id = 2;
        first.next_task_id = 2;
        loaded
            .database
            .apply_iroh_snapshot(board_id, "Shared", "viewer", 1, &first, None)
            .unwrap();
        let empty = StoredData::default();
        loaded
            .database
            .apply_iroh_snapshot(board_id, "Shared", "viewer", 2, &empty, None)
            .unwrap();
        assert!(loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns
            .is_empty());
        assert_eq!(loaded.database.boards().unwrap()[0].task_count, 0);
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_edit_revision_is_atomic_and_viewer_cannot_write() {
        let dir = test_dir("shared-local-revision");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let board_id = loaded.database.active_board_id();
        let before = loaded.database.read_board_complete(board_id).unwrap();
        let mut after = before.clone();
        after.columns.push(Column {
            id: 1,
            name: "Added".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![],
        });
        after.next_column_id = 2;
        loaded.database.persist_diff(&before, &after).unwrap();
        assert_eq!(loaded.database.board_sync_state(board_id).unwrap().0, 1);
        loaded
            .database
            .set_shared_board(board_id, "viewer", "synced", 1)
            .unwrap();
        let mut blocked = after.clone();
        blocked.columns.clear();
        assert!(loaded.database.persist_diff(&after, &blocked).is_err());
        assert_eq!(loaded.database.board_sync_state(board_id).unwrap().0, 1);
        assert_eq!(
            loaded
                .database
                .read_board_complete(board_id)
                .unwrap()
                .columns
                .len(),
            1
        );
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn conflict_keeps_local_board_and_using_remote_creates_a_copy() {
        let dir = test_dir("shared-conflict-copy");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let board_id = loaded.database.active_board_id();
        let mut local = StoredData::default();
        local.columns.push(Column {
            id: 1,
            name: "Local".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![],
        });
        let mut remote = StoredData::default();
        remote.columns.push(Column {
            id: 2,
            name: "Owner".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![],
        });
        let local_doc =
            crate::loro_board::apply_local_delta(None, &StoredData::default(), &local).unwrap();
        loaded
            .database
            .apply_iroh_snapshot(board_id, "Shared", "editor", 3, &local, Some(&local_doc))
            .unwrap();
        loaded
            .database
            .save_iroh_conflict(board_id, 4, "Shared", "editor", &remote)
            .unwrap();
        let mut newer_local = local.clone();
        newer_local.columns[0].name = "Local edited after conflict".into();
        loaded.database.persist_diff(&local, &newer_local).unwrap();
        assert_eq!(
            loaded.database.board_sync_state(board_id).unwrap().1,
            "conflict"
        );
        assert_eq!(
            loaded
                .database
                .read_board_complete(board_id)
                .unwrap()
                .columns[0]
                .name,
            "Local edited after conflict"
        );
        let saved = loaded.database.iroh_conflict(board_id).unwrap().unwrap();
        let copy = loaded
            .database
            .use_iroh_conflict_remote(
                board_id,
                "Shared",
                &newer_local,
                &saved.1,
                &saved.2,
                saved.0,
                &saved.3,
            )
            .unwrap();
        assert_eq!(
            loaded
                .database
                .read_board_complete(copy.id)
                .unwrap()
                .columns[0]
                .name,
            "Local edited after conflict"
        );
        assert_eq!(
            loaded
                .database
                .read_board_complete(board_id)
                .unwrap()
                .columns[0]
                .name,
            "Owner"
        );
        assert!(loaded.database.iroh_conflict(board_id).unwrap().is_none());
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "cardbe-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn notes_are_created_updated_sorted_and_deleted() {
        let dir = test_dir("notes");
        fs::create_dir_all(&dir).unwrap();
        let mut database = Database::open(dir.join("data.sqlite3")).unwrap();

        let older = database.create_note(100).unwrap();
        let newer = database
            .create_note_with_content(200, "Captured".into(), "From quick add".into())
            .unwrap();
        assert_eq!(newer.title, "Captured");
        assert_eq!(newer.content, "From quick add");
        let mut pinned = older.clone();
        pinned.title = "Pinned idea".into();
        pinned.content = "Keep this at the top".into();
        pinned.pinned = true;
        pinned.updated_at = 300;
        database.update_note(&pinned).unwrap();

        let notes = database.get_notes().unwrap();
        assert_eq!(notes, vec![pinned.clone(), newer.clone()]);

        database.delete_note(pinned.id).unwrap();
        assert_eq!(database.get_notes().unwrap(), vec![newer]);
        drop(database);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_json_and_preserves_the_source() {
        let dir = test_dir("sqlite-migration");
        fs::create_dir_all(&dir).unwrap();
        let mut stored = StoredData::default();
        stored.columns.push(Column {
            id: 0,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::DueDateDesc,
            tasks: vec![Task {
                id: 0,
                title: "Migrated".into(),
                ..Task::default()
            }],
        });
        stored.next_column_id = 1;
        stored.next_task_id = 1;
        write_document(&dir.join("data.json"), &stored).unwrap();

        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.stored, stored);
        assert_eq!(loaded.database.boards().unwrap().len(), 1);
        assert_eq!(loaded.database.boards().unwrap()[0].name, "My board");
        assert!(dir.join("data.sqlite3").exists());
        assert!(dir.join("data.migrated.json").exists());
        drop(loaded);

        let reloaded = load(&dir).unwrap();
        assert_eq!(reloaded.stored, stored);
        drop(reloaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn incomplete_database_creation_retries_json_migration() {
        let dir = test_dir("interrupted-migration");
        fs::create_dir_all(&dir).unwrap();
        let mut stored = StoredData::default();
        stored.columns.push(Column {
            id: 0,
            name: "Still here".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: Vec::new(),
        });
        stored.next_column_id = 1;
        write_document(&dir.join("data.json"), &stored).unwrap();

        // Opening creates the SQLite file and schema, but intentionally does
        // not commit imported application data or the initialization marker.
        drop(Database::open(dir.join("data.sqlite3")).unwrap());

        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.stored, stored);
        assert!(loaded.database.is_initialized().unwrap());
        assert!(dir.join("data.migrated.json").exists());
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_json_migration_rolls_back_and_can_be_retried() {
        let dir = test_dir("failed-migration-retry");
        fs::create_dir_all(&dir).unwrap();
        let mut stored = StoredData::default();
        stored.columns.push(Column {
            id: 0,
            name: "Must survive".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: Vec::new(),
        });
        stored.next_column_id = 1;
        write_document(&dir.join("data.json"), &stored).unwrap();

        let database_path = dir.join("data.sqlite3");
        let database = Database::open(database_path.clone()).unwrap();
        database
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_migration
                 BEFORE INSERT ON columns
                 BEGIN
                   SELECT RAISE(ABORT, 'simulated migration failure');
                 END;",
            )
            .unwrap();
        drop(database);

        let error = match load(&dir) {
            Ok(_) => panic!("migration should fail"),
            Err(error) => error,
        };
        assert!(error
            .to_string()
            .contains("source files were left unchanged"));
        assert!(dir.join("data.json").exists());
        assert!(!dir.join("data.migrated.json").exists());
        let database = Database::open(database_path).unwrap();
        assert!(!database.is_initialized().unwrap());
        assert!(database.load().unwrap().columns.is_empty());
        database
            .connection
            .execute_batch("DROP TRIGGER reject_migration;")
            .unwrap();
        drop(database);

        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.stored, stored);
        assert!(loaded.database.is_initialized().unwrap());
        assert!(dir.join("data.migrated.json").exists());
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn updating_one_task_only_persists_that_task() {
        let dir = test_dir("partial-update");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let mut before = StoredData::default();
        before.columns.push(Column {
            id: 0,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![
                Task {
                    id: 0,
                    title: "First".into(),
                    ..Task::default()
                },
                Task {
                    id: 1,
                    title: "Second".into(),
                    ..Task::default()
                },
            ],
        });
        before.next_column_id = 1;
        before.next_task_id = 2;
        loaded.database.replace_all(&before).unwrap();
        let mut after = before.clone();
        after.columns[0].tasks[1].title = "Changed".into();

        let stats = loaded.database.persist_diff(&before, &after).unwrap();
        assert_eq!(stats.tasks_changed, 1);
        assert_eq!(stats.columns_changed, 0);
        assert_eq!(stats.metadata_changed, 0);
        assert_eq!(loaded.database.load().unwrap(), after);
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moving_a_task_only_updates_the_moved_row() {
        let dir = test_dir("sparse-task-move");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let mut before = StoredData::default();
        before.columns.push(Column {
            id: 0,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: (0..5)
                .map(|id| Task {
                    id,
                    title: format!("Task {id}"),
                    ..Task::default()
                })
                .collect(),
        });
        before.next_column_id = 1;
        before.next_task_id = 5;
        loaded.database.replace_all(&before).unwrap();

        let mut after = before.clone();
        let moved = after.columns[0].tasks.remove(0);
        after.columns[0].tasks.push(moved);
        let stats = loaded.database.persist_diff(&before, &after).unwrap();

        assert_eq!(stats.tasks_changed, 1);
        assert_eq!(loaded.database.load().unwrap(), after);
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn database_uses_normal_synchronous_mode() {
        let dir = test_dir("normal-synchronous");
        fs::create_dir_all(&dir).unwrap();
        let database = Database::open(dir.join("data.sqlite3")).unwrap();
        let synchronous: i64 = database
            .connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 1);
        drop(database);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn conflicting_text_boards_table_is_preserved_and_ignored() {
        let dir = test_dir("foreign-boards-table");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.sqlite3");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE boards(id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO boards(id,name) VALUES ('foreign-id','Foreign board');",
                )
                .unwrap();
        }
        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.database.boards().unwrap().len(), 1);
        let foreign: (String, String) = loaded
            .database
            .connection
            .query_row("SELECT id,name FROM boards", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(foreign, ("foreign-id".into(), "Foreign board".into()));
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn archives_are_loaded_on_demand_after_initialization() {
        let dir = test_dir("lazy-archives");
        fs::create_dir_all(&dir).unwrap();
        let mut first = load(&dir).unwrap();
        let before = first.stored.clone();
        let mut after = before.clone();
        after.archives.push(Archive {
            time: 123,
            task: Task {
                id: 42,
                title: "Archived".into(),
                ..Task::default()
            },
        });
        after.next_task_id = 43;
        first.database.persist_diff(&before, &after).unwrap();
        drop(first);

        let second = load(&dir).unwrap();
        assert!(!second.archives_loaded);
        assert!(second.stored.archives.is_empty());
        assert_eq!(second.database.load_archives().unwrap(), after.archives);
        drop(second);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_transaction_keeps_the_previous_complete_state() {
        let dir = test_dir("transaction-rollback");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let mut before = StoredData::default();
        before.columns.push(Column {
            id: 0,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![Task {
                id: 0,
                title: "Safe".into(),
                ..Task::default()
            }],
        });
        before.next_column_id = 1;
        before.next_task_id = 1;
        loaded.database.replace_all(&before).unwrap();
        loaded
            .database
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_task_update
                 BEFORE UPDATE OF payload ON cardbe_tasks
                 BEGIN
                   SELECT RAISE(ABORT, 'simulated write failure');
                 END;",
            )
            .unwrap();

        let mut after = before.clone();
        after.columns[0].tasks[0].title = "Must roll back".into();
        assert!(loaded.database.persist_diff(&before, &after).is_err());
        assert_eq!(loaded.database.load().unwrap(), before);
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_json_recovers_from_backup_before_migration() {
        let dir = test_dir("backup-migration");
        fs::create_dir_all(&dir).unwrap();
        let mut stored = StoredData::default();
        stored.columns.push(Column {
            id: 0,
            name: "Recovered".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: Vec::new(),
        });
        write_document(&backup_path(&dir.join("data.json")), &stored).unwrap();
        fs::write(dir.join("data.json"), "not json").unwrap();

        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.stored.columns[0].name, "Recovered");
        assert!(!loaded.recovery_messages.is_empty());
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_current_json_does_not_resurrect_stale_legacy_data() {
        let dir = test_dir("no-stale-legacy");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("data.json"), "not json").unwrap();
        fs::write(
            dir.join("tasks.json"),
            r#"[{"name":"Stale","color":"","tasks":[]}]"#,
        )
        .unwrap();

        let loaded = load(&dir).unwrap();
        assert!(loaded.stored.columns.is_empty());
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn boards_isolate_same_ids_and_restore_active_board() {
        let dir = test_dir("board-isolation");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let first_id = loaded.database.active_board_id();
        let second = loaded.database.create_board("Second").unwrap();

        let first = loaded.stored.clone();
        let mut first_changed = first.clone();
        first_changed.columns.push(Column {
            id: 0,
            name: "First".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![Task {
                id: 0,
                title: "Only first".into(),
                ..Task::default()
            }],
        });
        first_changed.next_column_id = 1;
        first_changed.next_task_id = 1;
        loaded
            .database
            .persist_diff(&first, &first_changed)
            .unwrap();

        let second_before = loaded.database.load_board(second.id).unwrap();
        let mut second_changed = second_before.clone();
        second_changed.columns.push(Column {
            id: 0,
            name: "Second".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![Task {
                id: 0,
                title: "Only second".into(),
                ..Task::default()
            }],
        });
        second_changed.next_column_id = 1;
        second_changed.archives.push(Archive {
            time: 456,
            task: Task {
                id: 1,
                title: "Archived second".into(),
                ..Task::default()
            },
        });
        second_changed.next_task_id = 2;
        loaded
            .database
            .persist_diff(&second_before, &second_changed)
            .unwrap();

        let summaries = loaded.database.boards().unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].task_count, 1);
        assert_eq!(summaries[1].task_count, 1);

        let first_again = loaded.database.load_board(first_id).unwrap();
        assert_eq!(first_again.columns[0].name, "First");
        assert_eq!(first_again.columns[0].tasks[0].title, "Only first");
        drop(loaded);

        let reopened = load(&dir).unwrap();
        assert_eq!(reopened.database.active_board_id(), first_id);
        assert_eq!(reopened.stored.columns[0].name, "First");
        drop(reopened);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replace_all_boards_round_trips_same_logical_ids_and_global_data() {
        let dir = test_dir("all-board-round-trip");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();

        let mut first = StoredData::default();
        first.columns.push(Column {
            id: 0,
            name: "First column".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![Task {
                id: 0,
                title: "First task".into(),
                ..Task::default()
            }],
        });
        first.next_column_id = 1;
        first.next_task_id = 1;
        let mut second = first.clone();
        second.columns[0].name = "Second column".into();
        second.columns[0].tasks[0].title = "Second task".into();
        let settings = crate::models::Settings {
            notify_enabled: true,
            global_shortcuts_enabled: false,
        };
        let notes = vec![Note {
            id: 88,
            title: "Global note".into(),
            content: "Preserved across boards".into(),
            pinned: true,
            created_at: 1,
            updated_at: 2,
        }];

        let (boards, active) = loaded
            .database
            .replace_all_boards(
                &[("First".into(), first), ("Second".into(), second)],
                1,
                &notes,
                &settings,
            )
            .unwrap();

        assert_eq!(boards.len(), 2);
        assert_eq!(active.columns[0].tasks[0].title, "Second task");
        assert_eq!(loaded.database.active_board_id(), boards[1].id);
        assert_eq!(loaded.database.get_notes().unwrap(), notes);
        let restored_first = loaded.database.read_board(boards[0].id).unwrap();
        let restored_second = loaded.database.read_board(boards[1].id).unwrap();
        assert_eq!(restored_first.columns[0].id, restored_second.columns[0].id);
        assert_eq!(
            restored_first.columns[0].tasks[0].id,
            restored_second.columns[0].tasks[0].id
        );
        assert_eq!(restored_first.columns[0].tasks[0].title, "First task");
        assert_eq!(restored_second.columns[0].tasks[0].title, "Second task");
        assert_eq!(restored_first.settings, settings);

        drop(loaded);
        let reopened = load(&dir).unwrap();
        assert_eq!(reopened.database.active_board_id(), boards[1].id);
        assert_eq!(reopened.stored.columns[0].tasks[0].title, "Second task");
        assert_eq!(reopened.database.get_notes().unwrap(), notes);
        drop(reopened);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replace_all_boards_rolls_back_everything_when_notes_violate_a_constraint() {
        let dir = test_dir("all-board-rollback");
        fs::create_dir_all(&dir).unwrap();
        let mut loaded = load(&dir).unwrap();
        let original_id = loaded.database.active_board_id();
        let original = loaded.database.read_board(original_id).unwrap();
        let original_note = loaded
            .database
            .create_note_with_content(1, "Original".into(), "Keep me".into())
            .unwrap();
        let invalid_notes = vec![original_note.clone(), original_note.clone()];

        let result = loaded.database.replace_all_boards(
            &[("Replacement".into(), StoredData::default())],
            0,
            &invalid_notes,
            &crate::models::Settings {
                notify_enabled: true,
                global_shortcuts_enabled: false,
            },
        );

        assert!(result.is_err());
        assert_eq!(loaded.database.active_board_id(), original_id);
        assert_eq!(loaded.database.boards().unwrap().len(), 1);
        assert_eq!(loaded.database.read_board(original_id).unwrap(), original);
        assert_eq!(loaded.database.get_notes().unwrap(), vec![original_note]);
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }
}
