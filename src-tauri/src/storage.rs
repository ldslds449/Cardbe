use crate::errors::DomainError;
use crate::models::{
    Archive, Board, BoardRole, Column, ColumnOverview, ColumnSort, IrohDeviceStatus,
    IrohPermission, Note, StoredData, SyncStatus, Task, TaskSummary, TaskTemplate,
    CURRENT_SCHEMA_VERSION,
};
use crate::task_events::{self, EventSource};
use base64::Engine;
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef},
    Connection, OptionalExtension, Transaction, TransactionBehavior,
};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) type StorageResult<T> = Result<T, Box<dyn std::error::Error>>;

/// A task found for due notifications, with where it is.
pub struct DueTask {
    pub board_id: i64,
    pub board_name: String,
    pub column_id: i64,
    pub task: Task,
}
type IrohInviteRecord = (String, i64, String, IrohPermission, bool);
type IrohInviteSummaryRecord = (String, i64, IrohPermission, bool, String, String);

pub(crate) struct AllTaskRow {
    pub board_id: i64,
    pub board_name: String,
    pub column_id: Option<i64>,
    pub column_name: Option<String>,
    pub archived_at: Option<u128>,
    /// Whether the task is in its board's done column.
    pub done: bool,
    /// Open tasks of its board that the task still waits on.
    pub waiting_count: i64,
    pub task: TaskSummary,
    /// Where a search matched outside the title.
    pub excerpt: Option<String>,
}

/// Tasks matching a filter under each explorer status.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct TaskStatusCounts {
    pub all: i64,
    pub active: i64,
    pub overdue: i64,
    pub recurring: i64,
    pub archived: i64,
}

#[derive(Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct TaskExplorerQuery {
    pub query: String,
    pub board_ids: Option<Vec<i64>>,
    pub column_id: Option<i64>,
    pub status: String,
    pub sort: String,
    pub due: String,
    pub due_start: Option<i64>,
    pub due_end: Option<i64>,
    pub archive_start: Option<i64>,
    pub archive_end: Option<i64>,
    pub now: i64,
    /// Priorities to keep, with `none` for tasks without one; `None` keeps all.
    pub priorities: Option<Vec<String>>,
    /// `open`, `done` (in the done column) and `archived` tasks to keep;
    /// `None` keeps all. Applies on top of `status`.
    pub completion: Option<Vec<String>>,
    /// Keeps tasks with any of these labels.
    pub labels: Option<Vec<String>>,
    /// `waiting` keeps active tasks that wait on an open task, `not_waiting`
    /// the rest, and `waited_on` tasks that an open task waits on.
    pub waiting: Option<String>,
    /// Keeps only recurring tasks, or only the others.
    pub recurring: Option<bool>,
    /// Keeps tasks in any of these board columns, which leaves archived
    /// tasks out; `None` keeps all.
    pub columns: Option<Vec<crate::focuses::ColumnRef>>,
    pub search_options: Option<crate::search::SearchOptions>,
}

impl TaskExplorerQuery {
    /// The filters added for Focuses, passed to SQL as one JSON object.
    fn extra_filters(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&serde_json::json!({
            "completion": self.completion,
            "labels": self.labels,
            "waiting": self.waiting,
            "recurring": self.recurring,
            "columns": self.columns,
        }))
    }
}

// Shared eligibility is applied before either engine selects its Top K.
const TASK_SEARCH_CTE: &str = "WITH tasks AS (
             SELECT s.rowid AS search_id, s.board_id, b.name AS board_name, c.name AS column_name,
                    t.column_id, s.archived, s.task_id, a.archived_at,
                    CASE WHEN s.archived=0 THEN t.payload ELSE a.payload END AS payload,
                    (SELECT d.id FROM cardbe_columns d WHERE d.board_id=s.board_id AND d.done=1
                     ORDER BY d.position, d.id LIMIT 1) AS done_column_id
             FROM cardbe_task_search s
             JOIN cardbe_boards b ON b.id=s.board_id
             LEFT JOIN cardbe_tasks t
               ON s.archived=0 AND t.board_id=s.board_id AND t.id=s.task_id
             LEFT JOIN cardbe_columns c
               ON s.archived=0 AND c.board_id=t.board_id AND c.id=t.column_id
             LEFT JOIN cardbe_archives a
               ON s.archived=1 AND a.board_id=s.board_id AND a.task_id=s.task_id
             WHERE (?2 IS NULL OR s.board_id IN (SELECT value FROM json_each(?2)))
             ), summaries AS (
               SELECT *, json_extract(payload, '$.due_time') AS due_time,
                      json_extract(payload, '$.title') AS title,
                      json_extract(payload, '$.recurrence') AS recurrence,
                      COALESCE(json_extract(payload, '$.due_at_midnight'), 0) AS due_at_midnight,
                      COALESCE(json_extract(payload, '$.priority'), 'none') AS priority
               FROM tasks
             )
             , classified AS (SELECT *,
               CASE WHEN archived=1 THEN 'archived' WHEN column_id=done_column_id THEN 'done'
                    ELSE 'open' END AS completion,
               CASE WHEN archived=1 THEN 0 ELSE (
                 SELECT COUNT(*) FROM json_each(e.payload, '$.blocked_by') w
                 JOIN cardbe_tasks bt ON bt.board_id=e.board_id AND bt.id=w.value
                 WHERE bt.column_id IS NOT e.done_column_id) END AS waiting_count
             FROM summaries e)
             , eligible AS (SELECT * FROM classified c
             WHERE (?3 IS NULL OR column_id=?3)
               AND (CASE ?4
                 WHEN 'active' THEN archived=0
                 WHEN 'archived' THEN archived=1
                 WHEN 'overdue' THEN archived=0 AND due_time<?5 AND (due_time<>?11 OR due_at_midnight)
                 WHEN 'recurring' THEN archived=0 AND recurrence IS NOT NULL
                 ELSE 1 END)
               AND (CASE ?6 WHEN 'none' THEN due_time IS NULL
                           WHEN 'due' THEN due_time IS NOT NULL ELSE 1 END)
               AND (?7 IS NULL OR due_time>=?7)
               AND (?8 IS NULL OR due_time<?8)
               AND (archived=0 OR ((?9 IS NULL OR CAST(archived_at AS INTEGER)>=?9)
                                 AND (?10 IS NULL OR CAST(archived_at AS INTEGER)<?10)))
               AND (?12 IS NULL OR priority IN (SELECT value FROM json_each(?12)))
               AND (json_extract(?13, '$.completion') IS NULL
                    OR completion IN (SELECT value FROM json_each(?13, '$.completion')))
               AND (json_extract(?13, '$.labels') IS NULL OR EXISTS (
                 SELECT 1 FROM json_each(c.payload, '$.labels') l
                 WHERE l.value IN (SELECT value FROM json_each(?13, '$.labels'))))
               AND (json_extract(?13, '$.columns') IS NULL OR EXISTS (
                 SELECT 1 FROM json_each(?13, '$.columns') f
                 WHERE json_extract(f.value, '$.board_id')=c.board_id
                   AND json_extract(f.value, '$.column_id')=c.column_id))
               AND (json_extract(?13, '$.recurring') IS NULL
                    OR (recurrence IS NOT NULL)=json_extract(?13, '$.recurring'))
               AND (CASE json_extract(?13, '$.waiting')
                 WHEN 'waiting' THEN waiting_count>0
                 WHEN 'not_waiting' THEN waiting_count=0
                 WHEN 'waited_on' THEN completion='open' AND EXISTS (
                   SELECT 1 FROM cardbe_tasks wt, json_each(wt.payload, '$.blocked_by') w
                   WHERE wt.board_id=c.board_id AND w.value=c.task_id
                     AND wt.column_id IS NOT c.done_column_id)
                 ELSE 1 END)
)";

fn search_params(filter: &TaskExplorerQuery) -> StorageResult<Vec<rusqlite::types::Value>> {
    use rusqlite::types::Value;
    let opt = |value: Option<i64>| value.map(Value::Integer).unwrap_or(Value::Null);
    Ok(vec![
        Value::Null,
        filter
            .board_ids
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?
            .map(Value::Text)
            .unwrap_or(Value::Null),
        opt(filter.column_id),
        Value::Text(filter.status.clone()),
        Value::Integer(filter.now),
        Value::Text(filter.due.clone()),
        opt(filter.due_start),
        opt(filter.due_end),
        opt(filter.archive_start),
        opt(filter.archive_end),
        // A date-only due date today is stored at this midnight and is not
        // overdue until the day ends; earlier midnights already are. A chosen
        // 00:00 (`due_at_midnight`) is overdue as soon as it passes.
        Value::Integer(crate::task_dates::local_day_start(filter.now)),
        filter
            .priorities
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?
            .map(Value::Text)
            .unwrap_or(Value::Null),
        Value::Text(filter.extra_filters()?),
    ])
}

fn initialize_note_search_index(connection: &Connection) -> StorageResult<()> {
    let index_exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='cardbe_note_fts')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    connection.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS cardbe_note_fts USING fts5(
             title,content,content='notes',content_rowid='id',tokenize='trigram'
         );
         CREATE TRIGGER IF NOT EXISTS cardbe_notes_ai AFTER INSERT ON notes BEGIN
             INSERT INTO cardbe_note_fts(rowid,title,content) VALUES(new.id,new.title,new.content);
         END;
         CREATE TRIGGER IF NOT EXISTS cardbe_notes_ad AFTER DELETE ON notes BEGIN
             INSERT INTO cardbe_note_fts(cardbe_note_fts,rowid,title,content)
             VALUES('delete',old.id,old.title,old.content);
         END;
         CREATE TRIGGER IF NOT EXISTS cardbe_notes_au AFTER UPDATE OF id,title,content ON notes BEGIN
             INSERT INTO cardbe_note_fts(cardbe_note_fts,rowid,title,content)
             VALUES('delete',old.id,old.title,old.content);
             INSERT INTO cardbe_note_fts(rowid,title,content) VALUES(new.id,new.title,new.content);
         END;",
    )?;
    if !index_exists {
        connection
            .execute_batch("INSERT INTO cardbe_note_fts(cardbe_note_fts) VALUES('rebuild');")?;
    }
    Ok(())
}

fn hybrid_search(
    connection: &Connection,
    filter: &TaskExplorerQuery,
    options: crate::search::SearchOptions,
) -> StorageResult<Vec<i64>> {
    use crate::search::{fuse, title_priority, FuzzySearch};
    let query = filter.query.trim();
    if !options.valid() {
        return Err(DomainError::InvalidArgument.into());
    }
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let started = std::time::Instant::now();
    let lowercase_query = query.to_lowercase();
    let mut values = search_params(filter)?;
    values.push(rusqlite::types::Value::Text(if query.chars().count() < 3 {
        query.to_owned()
    } else {
        format!("\"{}\"", query.replace('"', "\"\""))
    }));
    values.push(rusqlite::types::Value::Integer(i64::try_from(
        options.candidate_limit,
    )?));
    let sql = if query.chars().count() < 3 {
        format!(
            "{TASK_SEARCH_CTE} SELECT e.search_id,e.title FROM eligible e
            JOIN cardbe_task_search s ON s.rowid=e.search_id
            WHERE instr(lower(s.body),lower(?14))>0 ORDER BY e.search_id LIMIT ?15"
        )
    } else {
        format!(
            "{TASK_SEARCH_CTE} SELECT e.search_id,e.title FROM cardbe_task_fts
            JOIN eligible e ON e.search_id=cardbe_task_fts.rowid
            WHERE cardbe_task_fts MATCH ?14 ORDER BY bm25(cardbe_task_fts),e.search_id LIMIT ?15"
        )
    };
    let mut statement = connection.prepare(&sql)?;
    let fts = statement
        .query_map(rusqlite::params_from_iter(&values), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                title_priority(&row.get::<_, String>(1)?, &lowercase_query),
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let fts_elapsed = started.elapsed();
    let fuzzy_started = std::time::Instant::now();
    // ponytail: large scopes scan every title; add a cache only after release profiling justifies it.
    let sql = format!("{TASK_SEARCH_CTE} SELECT search_id,title FROM eligible");
    let mut statement = connection.prepare(&sql)?;
    let values = search_params(filter)?;
    let mut rows = statement.query(rusqlite::params_from_iter(&values))?;
    let mut fuzzy = FuzzySearch::new(query, options.candidate_limit);
    let mut scanned = 0;
    while let Some(row) = rows.next()? {
        fuzzy.push(row.get(0)?, &row.get::<_, String>(1)?);
        scanned += 1;
    }
    let fuzzy = fuzzy.finish();
    let fuzzy_elapsed = fuzzy_started.elapsed();
    let merge_started = std::time::Instant::now();
    let results = fuse(&fts, &fuzzy, options);
    log::debug!(target: "storage.search", "scanned={} fts_us={} fuzzy_us={} merge_us={} total_us={} results={}",
        scanned, fts_elapsed.as_micros(), fuzzy_elapsed.as_micros(), merge_started.elapsed().as_micros(), started.elapsed().as_micros(), results.len());
    Ok(results)
}

pub struct IrohConflict {
    pub revision: i64,
    pub name: String,
    pub permission: IrohPermission,
    pub data: StoredData,
    pub loro_update: Option<Vec<u8>>,
}

macro_rules! impl_text_enum {
    ($type:ty { $($value:literal => $variant:path),+ $(,)? }) => {
        impl FromSql for $type {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                match value.as_str()? {
                    $($value => Ok($variant),)+
                    _ => Err(FromSqlError::InvalidType),
                }
            }
        }

        impl ToSql for $type {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                let value = match self {
                    $($variant => $value,)+
                };
                Ok(ToSqlOutput::from(value))
            }
        }
    };
}

impl_text_enum!(BoardRole {
    "owner" => BoardRole::Owner,
    "editor" => BoardRole::Editor,
    "viewer" => BoardRole::Viewer,
});
impl_text_enum!(SyncStatus {
    "local" => SyncStatus::Local,
    "pending" => SyncStatus::Pending,
    "synced" => SyncStatus::Synced,
    "conflict" => SyncStatus::Conflict,
});
impl_text_enum!(IrohPermission {
    "viewer" => IrohPermission::Viewer,
    "editor" => IrohPermission::Editor,
});

/// Reads `id, name, color, sort_order, sort_rules`.
fn column_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Column> {
    let sort_rules: String = row.get(4)?;
    let mut column = Column {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        sort_order: row.get(3)?,
        // Unreadable rules fall back to the legacy sort order.
        sort_rules: serde_json::from_str(&sort_rules).unwrap_or_default(),
        done: row.get(5)?,
        tasks: Vec::new(),
    };
    column.reconcile_sort();
    Ok(column)
}

impl FromSql for ColumnSort {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(match value.as_str()? {
            "due_date_asc" => Self::DueDateAsc,
            "due_date_desc" => Self::DueDateDesc,
            _ => Self::Custom,
        })
    }
}

impl ToSql for ColumnSort {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let value = match self {
            Self::Custom => "custom",
            Self::DueDateAsc => "due_date_asc",
            Self::DueDateDesc => "due_date_desc",
        };
        Ok(ToSqlOutput::from(value))
    }
}

impl FromSql for IrohDeviceStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_i64()? {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Approved),
            2 => Ok(Self::Revoked),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl ToSql for IrohDeviceStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let value: i64 = match self {
            Self::Pending => 0,
            Self::Approved => 1,
            Self::Revoked => 2,
        };
        Ok(ToSqlOutput::from(value))
    }
}

pub struct LoadedData {
    pub stored: StoredData,
    pub database: Database,
    pub recovery_messages: Vec<String>,
    pub archives_loaded: bool,
}

pub struct Database {
    pub(crate) connection: Connection,
    path: PathBuf,
    pub(crate) positions: DatabasePositions,
    active_board_id: i64,
    // Writer identities live only for this database session, never in backups.
    pub(crate) loro_peers: HashMap<i64, crate::loro_board::LocalPeer>,
}

#[derive(Clone, Default)]
pub(crate) struct DatabasePositions {
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
    /// The task event operation recorded for this write, if any.
    pub event_op: Option<i64>,
}

/// How a board write appears in the task event log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventWrite {
    /// Record task placement changes.
    Record(EventSource),
    /// The write replaces task ids, as an import does.
    Reset(EventSource),
}

impl Database {
    pub(crate) fn open_search_reader(path: PathBuf) -> StorageResult<Self> {
        let connection =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        // All search stages see one snapshot, without holding the application state lock.
        connection.execute_batch("BEGIN")?;
        Ok(Self {
            connection,
            path,
            positions: DatabasePositions::default(),
            active_board_id: 0,
            loro_peers: HashMap::new(),
        })
    }

    pub(crate) fn open(path: PathBuf) -> StorageResult<Self> {
        let connection = Connection::open(&path)?;
        crate::link_preview::cache::initialize(&connection)?;
        let sqlite_user_version: i64 =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        log::debug!(
            target: "storage.database",
            "Opened database path={} sqlite_user_version={} current_schema_version={}",
            path.display(),
            sqlite_user_version,
            CURRENT_SCHEMA_VERSION,
        );
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
        initialize_note_search_index(&connection)?;
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
        let _ = connection.execute(
            "ALTER TABLE columns ADD COLUMN sort_rules TEXT NOT NULL DEFAULT '[]'",
            [],
        );
        let positions = DatabasePositions::default();
        Ok(Self {
            connection,
            path,
            positions,
            active_board_id: 0,
            loro_peers: HashMap::new(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read_flow(
        &self,
        board_id: i64,
        tab_id: Option<&str>,
    ) -> StorageResult<crate::flow::FlowData> {
        crate::flow::read(&self.connection, board_id, tab_id)
    }

    pub fn read_flow_backup(&self, board_id: i64) -> StorageResult<crate::flow::FlowData> {
        crate::flow::read_backup(&self.connection, board_id)
    }

    pub fn save_flow_layout(
        &mut self,
        board_id: i64,
        tab_id: &str,
        layout: &crate::flow::FlowLayout,
    ) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        if !crate::flow::tabs(&tx, board_id)?
            .iter()
            .any(|tab| tab.id == tab_id)
        {
            return Err(DomainError::FlowTabNotFound.into());
        }
        crate::flow::validate_layout(layout, &crate::flow::task_ids(&tx, board_id)?)?;
        crate::flow::save_layout(&tx, board_id, tab_id, layout)?;
        tx.commit()?;
        Ok(())
    }

    pub fn change_flow_tabs(
        &mut self,
        board_id: i64,
        change: impl FnOnce(&Transaction<'_>) -> StorageResult<()>,
    ) -> StorageResult<Vec<crate::flow::FlowTab>> {
        let tx = self.connection.transaction()?;
        change(&tx)?;
        let tabs = crate::flow::tabs(&tx, board_id)?;
        tx.commit()?;
        Ok(tabs)
    }

    pub fn change_relationship(
        &mut self,
        board_id: i64,
        relationship: &crate::flow::Relationship,
        remove: bool,
    ) -> StorageResult<()> {
        let relationship = &crate::flow::normalize(relationship.clone());
        let tx = self.connection.transaction()?;
        if remove {
            crate::flow::delete_relationship(&tx, board_id, relationship)?;
            crate::flow::remove_connection(&tx, board_id, relationship)?;
        } else {
            let data = crate::flow::read(&tx, board_id, None)?;
            crate::flow::validate_relationship(
                &data.relationships,
                relationship,
                &crate::flow::task_ids(&tx, board_id)?,
            )?;
            crate::flow::insert_relationship(&tx, board_id, relationship)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_relationship(
        &mut self,
        board_id: i64,
        relationship: &crate::flow::Relationship,
    ) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        crate::flow::update_relationship(
            &tx,
            board_id,
            &crate::flow::normalize(relationship.clone()),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn search_tasks(&self, board_id: i64, query: &str) -> StorageResult<Vec<i64>> {
        self.search_tasks_with_options(board_id, query, crate::search::SearchOptions::default())
    }

    pub fn search_tasks_with_options(
        &self,
        board_id: i64,
        query: &str,
        options: crate::search::SearchOptions,
    ) -> StorageResult<Vec<i64>> {
        let filter = TaskExplorerQuery {
            query: query.to_owned(),
            board_ids: Some(vec![board_id]),
            ..Default::default()
        };
        let rowids = hybrid_search(&self.connection, &filter, options)?;
        let mut statement = self.connection.prepare(
            "SELECT s.task_id FROM json_each(?1) ranked
                 JOIN cardbe_task_search s ON s.rowid=ranked.value
                 ORDER BY CAST(ranked.key AS INTEGER)",
        )?;
        let rows = statement.query_map([serde_json::to_string(&rowids)?], |row| {
            row.get::<_, i64>(0)
        })?;
        let mut ids = Vec::with_capacity(rowids.len());
        let mut seen = std::collections::HashSet::new();
        for id in rows {
            let id = id?;
            if seen.insert(id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    fn initialize_task_search_index(&self) -> StorageResult<()> {
        let index_exists = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='cardbe_task_search')",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS cardbe_task_search(
                 rowid INTEGER PRIMARY KEY,
                 board_id INTEGER NOT NULL,
                 task_id INTEGER NOT NULL,
                 archived INTEGER NOT NULL,
                 body TEXT NOT NULL,
                 UNIQUE(board_id,task_id,archived)
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS cardbe_task_fts USING fts5(
                 body,content='cardbe_task_search',content_rowid='rowid',tokenize='trigram'
             );
             CREATE TRIGGER IF NOT EXISTS cardbe_task_search_ai AFTER INSERT ON cardbe_task_search BEGIN
                 INSERT INTO cardbe_task_fts(rowid,body) VALUES(new.rowid,new.body);
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_task_search_ad AFTER DELETE ON cardbe_task_search BEGIN
                 INSERT INTO cardbe_task_fts(cardbe_task_fts,rowid,body) VALUES('delete',old.rowid,old.body);
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_task_search_au AFTER UPDATE ON cardbe_task_search BEGIN
                 INSERT INTO cardbe_task_fts(cardbe_task_fts,rowid,body) VALUES('delete',old.rowid,old.body);
                 INSERT INTO cardbe_task_fts(rowid,body) VALUES(new.rowid,new.body);
             END;
             CREATE VIEW IF NOT EXISTS cardbe_task_search_payloads AS
                 SELECT board_id,id AS task_id,0 AS archived,payload FROM cardbe_tasks
                 UNION ALL
                 SELECT board_id,task_id,1 AS archived,payload FROM cardbe_archives;
             CREATE VIEW IF NOT EXISTS cardbe_task_search_content AS
                 SELECT board_id,task_id,archived,
                     COALESCE(json_extract(payload,'$.title'),'') || ' ' ||
                     COALESCE(json_extract(payload,'$.description'),'') || ' ' ||
                     COALESCE((SELECT group_concat(value,' ') FROM json_each(payload,'$.labels')),'') || ' ' ||
                     COALESCE((SELECT group_concat(json_extract(value,'$.text'),' ') FROM json_each(payload,'$.items')),'') AS body
                 FROM cardbe_task_search_payloads;
             CREATE TRIGGER IF NOT EXISTS cardbe_tasks_ai AFTER INSERT ON cardbe_tasks BEGIN
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.id AND archived=0
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_tasks_ad AFTER DELETE ON cardbe_tasks BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.id AND archived=0;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_tasks_au AFTER UPDATE OF board_id,id,payload ON cardbe_tasks BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.id AND archived=0;
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.id AND archived=0
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_archives_ai AFTER INSERT ON cardbe_archives BEGIN
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.task_id AND archived=1
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_archives_ad AFTER DELETE ON cardbe_archives BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.task_id AND archived=1;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_archives_au AFTER UPDATE OF board_id,task_id,payload ON cardbe_archives BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.task_id AND archived=1;
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.task_id AND archived=1
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;",
        )?;
        if !index_exists {
            self.connection.execute_batch(
                "INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content WHERE 1;",
            )?;
        }
        Ok(())
    }

    /// Unarchived tasks with a due time in `[from, to)`, for due notifications.
    pub fn tasks_due_between(&self, from: u128, to: u128) -> StorageResult<Vec<DueTask>> {
        let bound = |time: u128| i64::try_from(time).unwrap_or(i64::MAX);
        let mut stmt = self.connection.prepare(
            // Tasks in a done column are completed and need no reminder.
            "SELECT t.board_id,b.name,t.column_id,t.payload
             FROM cardbe_tasks t JOIN cardbe_boards b ON b.id=t.board_id
             JOIN cardbe_columns c ON c.board_id=t.board_id AND c.id=t.column_id
             WHERE c.done=0 AND json_extract(t.payload,'$.due_time')>=?1
               AND json_extract(t.payload,'$.due_time')<?2
             ORDER BY t.board_id,t.column_id,t.position,t.id",
        )?;
        let rows = stmt.query_map([bound(from), bound(to)], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut tasks = Vec::new();
        for row in rows {
            let (board_id, board_name, column_id, payload) = row?;
            tasks.push(DueTask {
                board_id,
                board_name,
                column_id,
                task: serde_json::from_str(&payload)?,
            });
        }
        Ok(tasks)
    }

    /// Due times before this instant (milliseconds since the Unix epoch) were
    /// already checked for notifications on this device.
    pub fn notifications_checked_until(&self) -> StorageResult<Option<u128>> {
        Ok(
            namespaced_metadata_value(&self.connection, "notifications_checked_until")?
                .and_then(|value| value.parse().ok()),
        )
    }

    pub fn set_notifications_checked_until(&mut self, time: u128) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES('notifications_checked_until',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [time.to_string()],
        )?;
        Ok(())
    }
    pub fn persist_diff(
        &mut self,
        before: &StoredData,
        after: &StoredData,
    ) -> StorageResult<PersistStats> {
        self.persist_diff_with_events(
            before,
            after,
            true,
            EventWrite::Record(EventSource::local()),
        )
    }

    #[cfg(test)]
    pub fn persist_diff_without_archives(
        &mut self,
        before: &StoredData,
        after: &StoredData,
    ) -> StorageResult<PersistStats> {
        self.persist_diff_with_events(
            before,
            after,
            false,
            EventWrite::Record(EventSource::local()),
        )
    }

    /// Persists the active board. Without loaded archives, `before` and
    /// `after` hold none, so archiving and restoring cannot occur.
    pub fn persist_diff_with_events(
        &mut self,
        before: &StoredData,
        after: &StoredData,
        include_archives: bool,
        events: EventWrite,
    ) -> StorageResult<PersistStats> {
        let mut positions = self.positions.clone();
        let board_changed = before.columns != after.columns
            || before.archives != after.archives
            || before.templates != after.templates
            || before.label_recency != after.label_recency
            || before.next_column_id != after.next_column_id
            || before.next_task_id != after.next_task_id
            || before.next_template_id != after.next_template_id
            || before.schema_version != after.schema_version;
        // Only a missing document needs the complete archive projection.
        let needs_archive_seed = !include_archives
            && board_changed
            && !self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM cardbe_loro_docs WHERE board_id=?1)",
                [self.active_board_id],
                |row| row.get::<_, bool>(0),
            )?;
        let archives = if needs_archive_seed {
            Some(self.load_board_archives(self.active_board_id)?)
        } else {
            None
        };
        let transaction = self.connection.transaction()?;
        if self.active_board_id == 0 {
            return Err("No active board is selected".into());
        }
        let role: BoardRole = transaction.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [self.active_board_id],
            |r| r.get(0),
        )?;
        if board_changed && role == BoardRole::Viewer {
            return Err(DomainError::PermissionDenied.into());
        }
        let mut stats = persist_board_diff_transaction(
            &transaction,
            self.active_board_id,
            before,
            after,
            &mut positions,
            include_archives,
        )?;
        if board_changed {
            stats.event_op = match &events {
                EventWrite::Record(source) => {
                    task_events::record(&transaction, self.active_board_id, before, after, source)?
                }
                EventWrite::Reset(source) => {
                    task_events::record_reset(&transaction, self.active_board_id, source)?
                }
            };
        }
        // Settings are global application preferences, not board content.
        transaction.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES ('settings',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [serde_json::to_string(&after.settings)?],
        )?;
        if board_changed {
            mark_local_board_change(&transaction, self.active_board_id, role)?;
            if matches!(role, BoardRole::Owner | BoardRole::Editor) {
                if let Some(archives) = archives {
                    let mut complete_before = before.clone();
                    let mut complete_after = after.clone();
                    complete_before.archives = archives.clone();
                    complete_after.archives = archives;
                    persist_loro_local_delta(
                        &transaction,
                        self.active_board_id,
                        &complete_before,
                        &complete_after,
                        self.loro_peers.entry(self.active_board_id).or_default(),
                    )?;
                } else {
                    persist_loro_local_delta(
                        &transaction,
                        self.active_board_id,
                        before,
                        after,
                        self.loro_peers.entry(self.active_board_id).or_default(),
                    )?;
                }
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

        let mut columns = self.connection.prepare(
            // Legacy single-board columns predate the done flag.
            "SELECT id, name, color, sort_order, sort_rules, 0 FROM columns ORDER BY position, id",
        )?;
        stored.columns = columns
            .query_map([], column_from_row)?
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
             CREATE TABLE IF NOT EXISTS cardbe_columns(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,id INTEGER NOT NULL,name TEXT NOT NULL,color TEXT NOT NULL,sort_order TEXT NOT NULL,position INTEGER NOT NULL,sort_rules TEXT NOT NULL DEFAULT '[]',done INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(board_id,id));
             CREATE TABLE IF NOT EXISTS cardbe_tasks(board_id INTEGER NOT NULL,id INTEGER NOT NULL,column_id INTEGER NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,id),FOREIGN KEY(board_id,column_id) REFERENCES cardbe_columns(board_id,id) ON DELETE CASCADE);
             CREATE INDEX IF NOT EXISTS cardbe_tasks_by_board_column_position ON cardbe_tasks(board_id,column_id,position);
             CREATE TABLE IF NOT EXISTS cardbe_archives(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,task_id INTEGER NOT NULL,archived_at TEXT NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,task_id));
             CREATE INDEX IF NOT EXISTS cardbe_archives_by_board_time ON cardbe_archives(board_id,CAST(archived_at AS INTEGER) DESC,task_id DESC);
             CREATE TABLE IF NOT EXISTS cardbe_templates(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,id INTEGER NOT NULL,name TEXT NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,id));
             CREATE TABLE IF NOT EXISTS cardbe_board_metadata(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,key TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(board_id,key));"
        )?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS cardbe_iroh_invites(invite_id TEXT PRIMARY KEY,board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,secret TEXT NOT NULL,permission TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1,created_at TEXT NOT NULL DEFAULT (datetime('now')));
            CREATE TABLE IF NOT EXISTS cardbe_iroh_devices(invite_id TEXT NOT NULL REFERENCES cardbe_iroh_invites(invite_id) ON DELETE CASCADE,node_id TEXT NOT NULL,status INTEGER NOT NULL DEFAULT 0 CHECK(status IN (0,1,2)),PRIMARY KEY(invite_id,node_id));
            CREATE TABLE IF NOT EXISTS cardbe_iroh_remotes(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,invitation TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cardbe_iroh_conflicts(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,revision INTEGER NOT NULL,name TEXT NOT NULL,permission TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cardbe_loro_docs(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,payload BLOB NOT NULL);")?;
        // Persist only an activation marker, never deleted invitation secrets.
        // The trigger also covers board cascades and backup restoration.
        tx.execute_batch("CREATE TRIGGER IF NOT EXISTS cardbe_iroh_record_revocation AFTER DELETE ON cardbe_iroh_invites BEGIN
            INSERT INTO cardbe_global_metadata(key,value) VALUES('iroh_has_revocations','1') ON CONFLICT(key) DO NOTHING;
            END;")?;
        for sql in [
            "ALTER TABLE cardbe_boards ADD COLUMN shared_role TEXT NOT NULL DEFAULT 'owner'",
            "ALTER TABLE cardbe_boards ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'local'",
            "ALTER TABLE cardbe_boards ADD COLUMN sync_revision INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE cardbe_iroh_invites ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE cardbe_iroh_invites ADD COLUMN created_at TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cardbe_iroh_conflicts ADD COLUMN loro_update BLOB",
            "ALTER TABLE cardbe_columns ADD COLUMN sort_rules TEXT NOT NULL DEFAULT '[]'",
            "ALTER TABLE cardbe_columns ADD COLUMN done INTEGER NOT NULL DEFAULT 0",
        ] {
            let _ = tx.execute(sql, []);
        }
        crate::flow::initialize(&tx)?;
        task_events::initialize(&tx)?;
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
        if let Some(value) = namespaced_metadata_json::<serde_json::Value>(&tx, "settings")? {
            if value.pointer("/iroh_network/enabled").is_none() {
                let mut settings: crate::models::Settings = serde_json::from_value(value)?;
                settings.iroh_network.enabled = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM cardbe_iroh_invites)
                     OR EXISTS(SELECT 1 FROM cardbe_iroh_remotes)
                     OR EXISTS(SELECT 1 FROM cardbe_global_metadata WHERE key='iroh_has_revocations')",
                    [],
                    |row| row.get(0),
                )?;
                tx.execute(
                    "UPDATE cardbe_global_metadata SET value=?1 WHERE key='settings'",
                    [serde_json::to_string(&settings)?],
                )?;
            }
        }
        tx.commit()?;
        self.initialize_task_search_index()?;
        Ok(())
    }

    fn write_board(&mut self, board_id: i64, data: &StoredData) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        write_board_transaction(&tx, board_id, data)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn load_board_positions(&self, board_id: i64) -> StorageResult<DatabasePositions> {
        Ok(DatabasePositions {
            columns: load_board_positions(&self.connection, "cardbe_columns", board_id)?,
            tasks: load_board_positions(&self.connection, "cardbe_tasks", board_id)?,
        })
    }

    pub fn list_all_task_page(
        &self,
        filter: &TaskExplorerQuery,
        offset: i64,
        limit: usize,
    ) -> StorageResult<Vec<AllTaskRow>> {
        if filter
            .search_options
            .is_some_and(|options| !options.valid())
        {
            return Err(DomainError::InvalidArgument.into());
        }
        // shortcut: OFFSET slows deep pages; use keyset cursors if profiling justifies it.
        let matches = if filter.query.trim().is_empty() {
            None
        } else {
            Some(serde_json::to_string(&hybrid_search(
                &self.connection,
                filter,
                filter.search_options.unwrap_or_default(),
            )?)?)
        };
        let sql = format!(
            "{TASK_SEARCH_CTE}
             , hits AS MATERIALIZED (
               SELECT CAST(value AS INTEGER) AS search_id, CAST(key AS INTEGER) AS rank
               FROM json_each(?1))
             SELECT e.board_id, e.board_name, e.column_name, e.archived_at, e.payload,
                    e.column_id, e.completion='done', e.waiting_count
             FROM eligible e
             LEFT JOIN hits h ON h.search_id=e.search_id
             WHERE (?1 IS NULL OR h.search_id IS NOT NULL)
             ORDER BY
               CASE WHEN ?14='relevance' THEN h.rank END,
               CASE WHEN ?14='priority' THEN CASE priority
                 WHEN 'urgent' THEN 4 WHEN 'high' THEN 3 WHEN 'medium' THEN 2
                 WHEN 'low' THEN 1 ELSE 0 END END DESC,
               CASE WHEN ?14 IN ('due','priority') THEN due_time IS NULL END,
               CASE WHEN ?14 IN ('due','priority') THEN due_time END,
               CASE WHEN ?14='archived' THEN CAST(archived_at AS INTEGER) END DESC,
               CASE WHEN ?14='column' THEN column_name END COLLATE NOCASE,
               CASE WHEN ?14='created' THEN
                 CAST(json_extract(payload, '$.created_time') AS INTEGER) END DESC,
               title COLLATE NOCASE, board_id, archived, task_id
             LIMIT ?15 OFFSET ?16",
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map(
                params![
                    matches,
                    filter
                        .board_ids
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    filter.column_id,
                    filter.status,
                    filter.now,
                    filter.due,
                    filter.due_start,
                    filter.due_end,
                    filter.archive_start,
                    filter.archive_end,
                    crate::task_dates::local_day_start(filter.now),
                    filter
                        .priorities
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    filter.extra_filters()?,
                    filter.sort,
                    i64::try_from(limit)?,
                    offset,
                ],
                |row| {
                    let board_id = row.get(0)?;
                    let board_name = row.get(1)?;
                    let column_name = row.get(2)?;
                    let archived_at = row
                        .get::<_, Option<String>>(3)?
                        .map(|value| value.parse::<u128>())
                        .transpose()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?;
                    let payload = row.get::<_, String>(4)?;
                    let task: Task = serde_json::from_str(&payload).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(AllTaskRow {
                        board_id,
                        board_name,
                        column_id: row.get(5)?,
                        column_name,
                        archived_at,
                        done: row.get(6)?,
                        waiting_count: row.get(7)?,
                        excerpt: crate::search::task_excerpt(&task, &filter.query),
                        task: TaskSummary::from(&task),
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Counts `filter`'s tasks under every status at once, ignoring its own
    /// status and sort. The column filter does not apply to archived tasks,
    /// as the explorer drops it there. Searches count one pool fused across
    /// all statuses, so each count is capped by the same search limits.
    pub fn count_all_tasks(&self, filter: &TaskExplorerQuery) -> StorageResult<TaskStatusCounts> {
        let every_status = TaskExplorerQuery {
            status: "all".into(),
            column_id: None,
            ..filter.clone()
        };
        let options = filter.search_options.unwrap_or_default();
        if !options.valid() {
            return Err(DomainError::InvalidArgument.into());
        }
        let mut values = search_params(&every_status)?;
        if !filter.query.trim().is_empty() {
            let matches = hybrid_search(&self.connection, &every_status, options)?;
            values[0] = rusqlite::types::Value::Text(serde_json::to_string(&matches)?);
        }
        values.push(
            filter
                .column_id
                .map(rusqlite::types::Value::Integer)
                .unwrap_or(rusqlite::types::Value::Null),
        );
        let sql = format!(
            "{TASK_SEARCH_CTE}
             , counted AS (SELECT *, archived=0 AND (?14 IS NULL OR column_id=?14) AS open_match
               FROM eligible
               WHERE (?1 IS NULL OR search_id IN (SELECT value FROM json_each(?1))))
             SELECT COALESCE(SUM(?14 IS NULL OR column_id=?14), 0),
                    COALESCE(SUM(open_match), 0),
                    COALESCE(SUM(open_match AND due_time<?5
                                 AND (due_time<>?11 OR due_at_midnight)), 0),
                    COALESCE(SUM(open_match AND recurrence IS NOT NULL), 0),
                    COALESCE(SUM(archived=1), 0)
             FROM counted",
        );
        Ok(self
            .connection
            .query_row(&sql, rusqlite::params_from_iter(&values), |row| {
                Ok(TaskStatusCounts {
                    all: row.get(0)?,
                    active: row.get(1)?,
                    overdue: row.get(2)?,
                    recurring: row.get(3)?,
                    archived: row.get(4)?,
                })
            })?)
    }

    pub fn task_events(
        &self,
        board_id: i64,
        task_id: Option<i64>,
    ) -> StorageResult<Vec<task_events::TaskEvent>> {
        task_events::list(&self.connection, board_id, task_id, None)
    }

    /// Board events recorded after `after_id`, for refreshing a loaded log.
    pub fn task_events_after(
        &self,
        board_id: i64,
        after_id: Option<i64>,
    ) -> StorageResult<Vec<task_events::TaskEvent>> {
        task_events::list(&self.connection, board_id, None, after_id)
    }

    pub fn load_task(&self, board_id: i64, task_id: i64) -> StorageResult<Option<Task>> {
        self.connection
            .query_row(
                "SELECT payload FROM cardbe_tasks WHERE board_id=?1 AND id=?2",
                params![board_id, task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
            .transpose()
    }
}

pub fn load(app_data_dir: &Path) -> StorageResult<LoadedData> {
    fs::create_dir_all(app_data_dir)?;
    let database_path = app_data_dir.join("data.sqlite3");
    let database_existed = database_path.try_exists()?;
    let mut messages = Vec::new();
    log::debug!(
        target: "storage.database",
        "Loading database path={} file_exists={} current_schema_version={}",
        database_path.display(),
        database_existed,
        CURRENT_SCHEMA_VERSION,
    );
    let mut database = Database::open(database_path)?;
    let storage_initialized = database.is_initialized()?;
    let legacy_schema_version = metadata_parse::<u32>(&database.connection, "schema_version")?;
    log::debug!(
        target: "storage.database",
        "Database state storage_initialized={} legacy_schema_version={:?}",
        storage_initialized,
        legacy_schema_version,
    );

    let (legacy, _archives_loaded) = if database_existed && storage_initialized {
        let archives_loaded = !database.lazy_archives_ready()?;
        // Snapshots must include archives even when the legacy store is still
        // using lazy archive hydration, otherwise the first board migration
        // would omit them.
        let stored = database.load()?;
        let (migrated, _) = stored.migrate().map_err(std::io::Error::other)?;
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
    let (boards, stored) = database.initialize_boards(&legacy)?;
    let before = stored.clone();
    let (mut stored, changed) = stored.migrate().map_err(std::io::Error::other)?;
    if changed {
        if database.board_role(database.active_board_id)? == BoardRole::Viewer {
            // Startup normalization is a local schema repair, not a user edit.
            // Keep the owner's permission, revision, and synchronization state.
            database.write_board(database.active_board_id, &stored)?;
            database.positions = database.load_board_positions(database.active_board_id)?;
        } else {
            database.persist_diff(&before, &stored)?;
        }
    }
    if !_archives_loaded {
        stored.archives.clear();
    }
    log::debug!(
        target: "storage.database",
        "Database ready board_count={} active_board_id={} schema_version={} schema_migrated={} archives_loaded={} recovery_messages={}",
        boards.len(),
        database.active_board_id(),
        stored.schema_version,
        changed,
        _archives_loaded,
        messages.len(),
    );
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
                || old.sort_rules != column.sort_rules
        });
        if changed {
            transaction.execute(
                "INSERT INTO columns(id, name, color, sort_order, sort_rules, position) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, color = excluded.color, sort_order = excluded.sort_order,
                   sort_rules = excluded.sort_rules, position = excluded.position",
                params![
                    id,
                    column.name,
                    column.color,
                    column.sort_order,
                    serde_json::to_string(&column.sort_rules)?,
                    position
                ],
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

pub(crate) fn persist_board_diff_transaction(
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
                || o.sort_rules != c.sort_rules
                || o.done != c.done
        }) {
            tx.execute("INSERT INTO cardbe_columns(board_id,id,name,color,sort_order,sort_rules,position,done) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(board_id,id) DO UPDATE SET name=excluded.name,color=excluded.color,sort_order=excluded.sort_order,sort_rules=excluded.sort_rules,position=excluded.position,done=excluded.done",params![board_id,id,c.name,c.color,c.sort_order,serde_json::to_string(&c.sort_rules)?,pos,c.done])?;
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
    crate::flow::prune(tx, board_id)?;
    positions.columns = column_positions;
    positions.tasks = task_positions;
    Ok(stats)
}

/// What `Database::read_iroh_share` reads beyond authorization and the board.
pub struct IrohShareRead {
    pub loro_document: bool,
    pub data: bool,
}

pub struct IrohShareContents {
    pub board: Board,
    pub permission: IrohPermission,
    pub loro_document: Option<Vec<u8>>,
    pub data: Option<StoredData>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IrohShareDenied {
    DeviceNotApproved,
    InvitationUnavailable,
    BoardDeleted,
}

/// An editor push merged against a specific Loro document (`base`).
/// `merged` is `None` when the push adds no operations.
pub struct PreparedLoroUpdate {
    board_id: i64,
    base: Vec<u8>,
    merged: Option<(Vec<u8>, StoredData)>,
}

fn ensure_iroh_editor(
    connection: &Connection,
    board_id: i64,
    invite_id: &str,
    secret: &str,
    node_id: &str,
) -> StorageResult<()> {
    let authorized: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM cardbe_iroh_invites i JOIN cardbe_iroh_devices d ON d.invite_id=i.invite_id WHERE i.invite_id=?1 AND i.board_id=?2 AND i.secret=?3 AND d.node_id=?4 AND d.status=?5 AND i.permission=?6 AND i.enabled=1)",
        params![invite_id, board_id, secret, node_id, IrohDeviceStatus::Approved, IrohPermission::Editor], |row| row.get(0),
    )?;
    if authorized {
        Ok(())
    } else {
        Err(DomainError::PermissionDenied.into())
    }
}

fn editable_loro_doc(connection: &Connection, board_id: i64) -> StorageResult<Vec<u8>> {
    connection
        .query_row(
            "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
            [board_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| "Editable board is missing its Loro document".into())
}

// Bounds for board state received from a peer. Local edits are unbounded, so
// a board that already exceeds a bound keeps syncing as long as a received
// change does not enlarge that measure further.
const REMOTE_MAX_COLUMNS: usize = 1_000;
const REMOTE_MAX_TASKS: usize = 100_000;
const REMOTE_MAX_TEMPLATES: usize = 1_000;
// Serialized size of the CRDT-owned content, excluding Loro edit history,
// which grows with normal use.
const REMOTE_MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;

fn task_count(data: &StoredData) -> usize {
    data.columns
        .iter()
        .map(|column| column.tasks.len())
        .sum::<usize>()
        + data.archives.len()
}

fn content_bytes(data: &StoredData) -> StorageResult<usize> {
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(
        &mut counter,
        &(&data.columns, &data.archives, &data.templates),
    )?;
    Ok(counter.0)
}

fn within_remote_bound(received: usize, previous: usize, bound: usize) -> bool {
    received <= bound || received <= previous
}

/// Reject received board state that grows beyond the remote bounds. Without a
/// previous local state, the bounds apply absolutely.
fn validate_remote_board_size(
    previous: Option<&StoredData>,
    received: &StoredData,
) -> StorageResult<()> {
    let (previous_columns, previous_tasks, previous_templates, previous_bytes) = match previous {
        Some(data) => (
            data.columns.len(),
            task_count(data),
            data.templates.len(),
            content_bytes(data)?,
        ),
        None => (0, 0, 0, 0),
    };
    if within_remote_bound(received.columns.len(), previous_columns, REMOTE_MAX_COLUMNS)
        && within_remote_bound(task_count(received), previous_tasks, REMOTE_MAX_TASKS)
        && within_remote_bound(
            received.templates.len(),
            previous_templates,
            REMOTE_MAX_TEMPLATES,
        )
        && within_remote_bound(
            content_bytes(received)?,
            previous_bytes,
            REMOTE_MAX_CONTENT_BYTES,
        )
    {
        Ok(())
    } else {
        Err(DomainError::ShareLimitExceeded.into())
    }
}

fn validate_iroh_snapshot(
    data: &StoredData,
    role: IrohPermission,
    loro_update: Option<&[u8]>,
) -> StorageResult<()> {
    validate_remote_board_size(None, data)?;
    if role == IrohPermission::Editor {
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
    Ok(())
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
        "DELETE FROM cardbe_board_metadata WHERE board_id=?1 AND key <> 'link_reference'",
        [board_id],
    )?;
    let mut p = DatabasePositions::default();
    persist_board_diff_transaction(tx, board_id, &StoredData::default(), data, &mut p, true)?;
    Ok(())
}

/// Persists a local edit committed outside the open board's Undo history
/// (plugin changesets and shared operations) and records it for synchronization.
#[allow(clippy::too_many_arguments)]
pub(crate) fn persist_local_board_change(
    tx: &Transaction<'_>,
    board_id: i64,
    role: BoardRole,
    before: &StoredData,
    after: &StoredData,
    positions: &mut DatabasePositions,
    peer: &mut crate::loro_board::LocalPeer,
    include_archives: bool,
    source: &EventSource,
) -> StorageResult<()> {
    persist_board_diff_transaction(tx, board_id, before, after, positions, include_archives)?;
    if before != after {
        mark_local_board_change(tx, board_id, role)?;
        persist_loro_local_delta(tx, board_id, before, after, peer)?;
        task_events::record(tx, board_id, before, after, source)?;
    }
    Ok(())
}

pub(crate) fn mark_local_board_change(
    tx: &Transaction<'_>,
    board_id: i64,
    role: BoardRole,
) -> StorageResult<()> {
    match role {
        BoardRole::Owner => {
            tx.execute(
                "UPDATE cardbe_boards SET sync_revision=sync_revision+1 WHERE id=?1",
                [board_id],
            )?;
        }
        BoardRole::Editor => {
            tx.execute(
                "UPDATE cardbe_boards SET sync_status=CASE WHEN sync_status=?2 THEN ?2 ELSE ?3 END WHERE id=?1",
                params![board_id, SyncStatus::Conflict, SyncStatus::Pending],
            )?;
        }
        _ => return Err(DomainError::PermissionDenied.into()),
    }
    Ok(())
}

pub(crate) fn persist_loro_local_delta(
    tx: &Transaction<'_>,
    board_id: i64,
    before: &StoredData,
    after: &StoredData,
    peer: &mut crate::loro_board::LocalPeer,
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
        crate::loro_board::apply_local_delta_with_peer(existing.as_deref(), before, after, peer)
    } else {
        crate::loro_board::apply_local_delta_with_peer(None, &StoredData::default(), after, peer)
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
    let sql = match table {
        "cardbe_columns" => "SELECT id,position FROM cardbe_columns WHERE board_id=?1",
        "cardbe_tasks" => "SELECT id,position FROM cardbe_tasks WHERE board_id=?1",
        _ => return Err(format!("Unsupported position table: {table}").into()),
    };
    let mut s = connection.prepare(sql)?;
    let result = s
        .query_map([board_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(result)
}

fn table_columns(tx: &Transaction<'_>, table: &str) -> StorageResult<Vec<(String, String)>> {
    let sql = match table {
        "cardbe_boards" => "PRAGMA table_info(cardbe_boards)",
        "cardbe_columns" => "PRAGMA table_info(cardbe_columns)",
        "cardbe_tasks" => "PRAGMA table_info(cardbe_tasks)",
        "boards" => "PRAGMA table_info(boards)",
        "board_snapshots" => "PRAGMA table_info(board_snapshots)",
        _ => return Err(format!("Unsupported schema table: {table}").into()),
    };
    let mut s = tx.prepare(sql)?;
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

mod boards;
mod iroh_store;
mod notes;

#[cfg(test)]
mod tests;
