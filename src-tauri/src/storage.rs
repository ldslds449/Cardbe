use crate::models::{Archive, Column, ColumnSort, Note, StoredData, Task, TaskTemplate};
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
             );",
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
        let positions = DatabasePositions {
            columns: load_positions(&connection, "columns", "id")?,
            tasks: load_positions(&connection, "tasks", "id")?,
        };
        Ok(Self {
            connection,
            path,
            positions,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
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
        let stats = persist_diff_transaction(
            &transaction,
            before,
            after,
            &mut positions,
            include_archives,
        )?;
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
        Ok(())
    }

    fn is_initialized(&self) -> StorageResult<bool> {
        Ok(metadata_value(&self.connection, "storage_initialized")?.as_deref() == Some("1"))
    }

    fn load(&self) -> StorageResult<StoredData> {
        self.load_internal(true)
    }

    fn load_without_archives(&self) -> StorageResult<StoredData> {
        self.load_internal(false)
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
}

pub fn load(app_data_dir: &Path) -> StorageResult<LoadedData> {
    fs::create_dir_all(app_data_dir)?;
    let database_path = app_data_dir.join("data.sqlite3");
    let database_existed = database_path.try_exists()?;
    let mut messages = Vec::new();
    let mut database = Database::open(database_path)?;

    let (stored, archives_loaded) = if database_existed && database.is_initialized()? {
        let archives_loaded = !database.lazy_archives_ready()?;
        let stored = if archives_loaded {
            database.load()?
        } else {
            database.load_without_archives()?
        };
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

    Ok(LoadedData {
        stored,
        database,
        recovery_messages: messages,
        archives_loaded,
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

fn load_positions(
    connection: &Connection,
    table: &str,
    id_column: &str,
) -> StorageResult<HashMap<i64, i64>> {
    let mut statement =
        connection.prepare(&format!("SELECT {id_column}, position FROM {table}"))?;
    let positions = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(positions)
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
                 BEFORE UPDATE OF payload ON tasks
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
}
