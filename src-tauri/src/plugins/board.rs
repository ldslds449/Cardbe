//! Board-scoped queries and validated changesets. No guest writes during execution.
use crate::{board_mutations as mutation, errors::DomainError, models::*, storage::Database};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

fn internal(error: impl std::fmt::Display) -> DomainError {
    DomainError::Internal(error.to_string())
}

/// Covers persisted content and order, including remote/editor writes that do
/// not increment the owner's sync_revision. No settings or secrets are hashed.
pub fn version(db: &Database, board: i64) -> Result<String, DomainError> {
    db.board_role(board).map_err(DomainError::repository)?;
    let mut hash = Sha256::new();
    for sql in [
        "SELECT json_array(name) FROM cardbe_boards WHERE id=?1",
        "SELECT json_array(id,name,color,sort_order,position) FROM cardbe_columns WHERE board_id=?1 ORDER BY id",
        "SELECT json_array(id,column_id,position,payload) FROM cardbe_tasks WHERE board_id=?1 ORDER BY id",
        "SELECT json_array(task_id,archived_at,position,payload) FROM cardbe_archives WHERE board_id=?1 ORDER BY task_id",
        "SELECT json_array(key,value) FROM cardbe_board_metadata WHERE board_id=?1 AND key='label_recency'",
    ] {
        let mut stmt = db.connection.prepare(sql).map_err(internal)?;
        for row in stmt.query_map([board], |r| r.get::<_, String>(0)).map_err(internal)? {
            let row = row.map_err(internal)?;
            hash.update((row.len() as u64).to_le_bytes());
            hash.update(row.as_bytes());
        }
        hash.update([0xff]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ColumnInfo {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub color: String,
    pub sort_order: ColumnSort,
    pub position: i64,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct TaskInfo {
    pub board_id: i64,
    pub column_id: Option<i64>,
    pub position: Option<i64>,
    pub archived: bool,
    pub task: Task,
}
pub struct Reader {
    pub db: Database,
    pub board_id: i64,
    pub version: String,
    now: i64,
}
impl Reader {
    pub fn open(path: std::path::PathBuf, board_id: i64) -> Result<Self, DomainError> {
        let db = Database::open_search_reader(path).map_err(DomainError::repository)?;
        let version = version(&db, board_id)?;
        Ok(Self {
            db,
            board_id,
            version,
            now: super::types::now(),
        })
    }
    pub fn get_board(&self) -> Result<Board, DomainError> {
        self.db
            .boards()
            .map_err(DomainError::repository)?
            .into_iter()
            .find(|b| b.id == self.board_id)
            .ok_or(DomainError::BoardNotFound)
    }
    pub fn list_boards(&self) -> Result<Vec<Board>, DomainError> {
        Ok(vec![self.get_board()?])
    }
    fn columns(&self, id: Option<i64>) -> Result<Vec<ColumnInfo>, DomainError> {
        let mut stmt = self.db.connection.prepare(
            "SELECT id,name,color,sort_order,position FROM cardbe_columns WHERE board_id=?1 AND (?2 IS NULL OR id=?2) ORDER BY position,id"
        ).map_err(internal)?;
        let rows = stmt
            .query_map(params![self.board_id, id], |r| {
                Ok(ColumnInfo {
                    id: r.get(0)?,
                    board_id: self.board_id,
                    name: r.get(1)?,
                    color: r.get(2)?,
                    sort_order: r.get(3)?,
                    position: r.get(4)?,
                })
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal);
        rows
    }
    pub fn list_columns(&self) -> Result<Vec<ColumnInfo>, DomainError> {
        self.columns(None)
    }
    pub fn get_column(&self, id: i64) -> Result<ColumnInfo, DomainError> {
        self.columns(Some(id))?
            .into_iter()
            .next()
            .ok_or(DomainError::ColumnNotFound)
    }
    pub fn get_task(&self, id: i64, archived: bool) -> Result<TaskInfo, DomainError> {
        let task = if archived {
            self.db.load_archive_task(self.board_id, id)
        } else {
            self.db.load_task(self.board_id, id)
        }
        .map_err(DomainError::repository)?
        .ok_or(DomainError::TaskNotFound)?;
        let location: Option<(i64, i64)> = if archived {
            None
        } else {
            self.db
                .connection
                .query_row(
                    "SELECT column_id,position FROM cardbe_tasks WHERE board_id=?1 AND id=?2",
                    params![self.board_id, id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(internal)?
        };
        Ok(TaskInfo {
            board_id: self.board_id,
            column_id: location.map(|x| x.0),
            position: location.map(|x| x.1),
            archived,
            task,
        })
    }
    pub fn list_tasks(
        &self,
        mut filter: crate::storage::TaskExplorerQuery,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<TaskInfo>, DomainError> {
        if !(1..=100).contains(&limit)
            || filter.board_ids.is_some()
            || filter.query.len() > 4096
            || !matches!(
                filter.status.as_str(),
                "" | "active" | "archived" | "overdue" | "recurring" | "all"
            )
            || !matches!(
                filter.sort.as_str(),
                "" | "title" | "due" | "column" | "archived"
            )
            || !matches!(filter.due.as_str(), "" | "none" | "due")
            || filter
                .search_options
                .is_some_and(|options| !options.valid())
        {
            return Err(DomainError::InvalidArgument);
        }
        filter.board_ids = Some(vec![self.board_id]);
        filter.now = self.now;
        self.db
            .list_all_task_page(&filter, offset.into(), limit as usize)
            .map_err(DomainError::repository)?
            .into_iter()
            .map(|r| self.get_task(r.task.id, r.archived_at.is_some()))
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Reference {
    Id(i64),
    Temporary(String),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    UpsertExternalTask {
        task: super::runtime::ExternalTask,
        #[serde(default)]
        column: Option<Reference>,
    },
    CreateColumn {
        key: String,
        name: String,
        color: String,
    },
    UpdateColumn {
        column: Reference,
        name: String,
        color: String,
        sort_order: ColumnSort,
    },
    MoveColumn {
        column: Reference,
        before: Option<Reference>,
    },
    CreateTask {
        key: String,
        column: Reference,
        task: Task,
    },
    UpdateTask {
        task_id: i64,
        task: Task,
    },
    MoveTask {
        task_id: i64,
        column: Reference,
        before_task_id: Option<i64>,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSet {
    pub version: String,
    pub operations: Vec<Operation>,
}
pub struct PreparedChangeSet {
    pub instance: super::types::PluginInstance,
    pub run_id: String,
    pub version: String,
    pub after: StoredData,
    pub(crate) sources: HashMap<String, super::changeset::SourceChange>,
    pub cursor: Option<Option<String>>,
    pub has_board_operations: bool,
    pub created: usize,
    pub updated: usize,
}
pub fn prepare_output(
    db: &Database,
    instance: super::types::PluginInstance,
    run_id: String,
    output: &super::runtime::RuntimeOutput,
) -> Result<PreparedChangeSet, DomainError> {
    let mut changeset = match output.changeset.as_ref() {
        Some(changeset) => changeset.clone(),
        None => ChangeSet {
            version: output.snapshot_version.clone(),
            operations: vec![],
        },
    };
    if changeset.version != output.snapshot_version {
        return Err(DomainError::StaleBoardRequest);
    }
    changeset.operations.extend(
        output
            .tasks
            .iter()
            .cloned()
            .map(|task| Operation::UpsertExternalTask { task, column: None }),
    );
    let mut prepared = prepare(db, instance, run_id, changeset)?;
    prepared.cursor = Some(output.cursor.clone());
    Ok(prepared)
}
fn resolve(reference: &Reference, columns: &HashMap<String, i64>) -> Result<i64, DomainError> {
    match reference {
        Reference::Id(id) => Ok(*id),
        Reference::Temporary(key) => columns
            .get(key)
            .copied()
            .ok_or(DomainError::InvalidArgument),
    }
}
fn valid_task(task: &Task) -> bool {
    !task.title.trim().is_empty()
        && task.title.len() <= 4096
        && task.description.len() <= 262144
        && task.color.len() <= 64
        && task.labels.len() <= 100
        && task.labels.iter().collect::<HashSet<_>>().len() == task.labels.len()
        && task
            .labels
            .iter()
            .all(|x| !x.trim().is_empty() && x.len() <= 256)
        && task.items.len() <= 1000
        && task
            .items
            .iter()
            .map(|item| &item.id)
            .collect::<HashSet<_>>()
            .len()
            == task.items.len()
        && task
            .items
            .iter()
            .all(|x| !x.id.is_empty() && x.id.len() <= 128 && x.text.len() <= 4096)
        && task.start_time <= 8_640_000_000_000_000
        && task.due_time.is_none_or(|x| x <= 8_640_000_000_000_000)
        && task
            .recurrence
            .as_ref()
            .is_none_or(|r| r.interval > 0 && r.interval <= 10000)
}
pub fn prepare(
    db: &Database,
    instance: super::types::PluginInstance,
    run_id: String,
    changeset: ChangeSet,
) -> Result<PreparedChangeSet, DomainError> {
    if serde_json::to_vec(&changeset).map_err(internal)?.len() > 8 * 1024 * 1024 {
        return Err(DomainError::InvalidArgument);
    }
    if db
        .board_role(instance.board_id)
        .map_err(DomainError::repository)?
        == BoardRole::Viewer
    {
        return Err(DomainError::PermissionDenied);
    }
    let upserts = changeset
        .operations
        .iter()
        .filter(|op| matches!(op, Operation::UpsertExternalTask { .. }))
        .count();
    if upserts > 2000 || changeset.operations.len() - upserts > 200 {
        return Err(DomainError::InvalidArgument);
    }
    if version(db, instance.board_id)? != changeset.version {
        return Err(DomainError::StaleBoardRequest);
    }
    let has_board_operations = !changeset.operations.is_empty();
    let mut after = if has_board_operations {
        db.read_board_complete(instance.board_id)
            .map_err(DomainError::repository)?
    } else {
        StoredData::default()
    };
    let mut ids = HashMap::new();
    let mut columns = HashMap::new();
    let mut sources = HashMap::new();
    let mut source_created = 0;
    let mut source_updated = 0;
    for op in &changeset.operations {
        match op {
            Operation::UpsertExternalTask { task, column } => {
                let mut incoming = task.clone();
                if let Some(column) = column {
                    incoming.column_id = resolve(column, &columns)?;
                }
                let (change, created, updated) = super::changeset::prepare_source(
                    db.plugin_connection(),
                    &instance,
                    &mut after,
                    &incoming,
                    &sources,
                )?;
                source_created += usize::from(created);
                source_updated += usize::from(updated);
                sources.insert(incoming.external_key.clone(), change);
            }
            Operation::CreateColumn { key, name, color } => {
                if key.is_empty()
                    || key.len() > 128
                    || ids.contains_key(key)
                    || name.trim().is_empty()
                    || name.len() > 4096
                    || color.len() > 64
                {
                    return Err(DomainError::InvalidArgument);
                }
                let id = mutation::add_column(&mut after, name.clone(), color.clone())?;
                ids.insert(key.clone(), id);
                columns.insert(key.clone(), id);
            }
            Operation::UpdateColumn {
                column,
                name,
                color,
                sort_order,
            } => {
                if name.trim().is_empty() || name.len() > 4096 || color.len() > 64 {
                    return Err(DomainError::InvalidArgument);
                }
                mutation::update_column(
                    &mut after,
                    resolve(column, &columns)?,
                    name.clone(),
                    color.clone(),
                    sort_order.clone(),
                )?;
            }
            Operation::MoveColumn { column, before } => {
                let id = resolve(column, &columns)?;
                mutation::column_index(&after, id)?;
                mutation::move_column(
                    &mut after,
                    id,
                    before.as_ref().map(|r| resolve(r, &columns)).transpose()?,
                )?;
            }
            Operation::CreateTask { key, column, task } => {
                if key.is_empty() || key.len() > 128 || ids.contains_key(key) || !valid_task(task) {
                    return Err(DomainError::InvalidArgument);
                }
                let id =
                    mutation::add_task(&mut after, resolve(column, &columns)?, task.clone(), None)?;
                ids.insert(key.clone(), id);
            }
            Operation::UpdateTask { task_id, task } => {
                if !valid_task(task) {
                    return Err(DomainError::InvalidArgument);
                }
                mutation::update_task(&mut after, *task_id, task.clone())?;
            }
            Operation::MoveTask {
                task_id,
                column,
                before_task_id,
            } => {
                let destination = resolve(column, &columns)?;
                let ci = mutation::column_index(&after, destination)?;
                if after.columns[ci].sort_order != ColumnSort::Custom {
                    return Err(DomainError::InvalidArgument);
                }
                mutation::task_position(&after, *task_id)?;
                if *before_task_id == Some(*task_id)
                    && after.columns[ci].tasks.iter().all(|t| t.id != *task_id)
                {
                    return Err(DomainError::InvalidArgument);
                }
                mutation::move_task(&mut after, *task_id, destination, *before_task_id)?;
            }
        }
    }
    after.sort_column_tasks();
    if serde_json::to_vec(&after.columns).map_err(internal)?.len() > 8 * 1024 * 1024 {
        return Err(DomainError::InvalidArgument);
    }
    let general_count = changeset.operations.len() - upserts;
    let general_created = ids.len();
    // Only the mutable projection is retained; archives and settings are read fresh at commit.
    let after = StoredData {
        columns: after.columns,
        label_recency: after.label_recency,
        ..StoredData::default()
    };
    Ok(PreparedChangeSet {
        instance,
        run_id,
        version: changeset.version,
        after,
        sources,
        has_board_operations,
        cursor: None,
        created: general_created + source_created,
        updated: general_count - general_created + source_updated,
    })
}

impl Database {
    #[cfg(test)]
    pub fn commit_changeset(
        &mut self,
        prepared: &PreparedChangeSet,
    ) -> Result<StoredData, DomainError> {
        self.commit_changeset_if(prepared, || true)
            .map(|stored| stored.expect("test prepares board operations"))
    }
    pub fn commit_changeset_if(
        &mut self,
        prepared: &PreparedChangeSet,
        may_commit: impl Fn() -> bool,
    ) -> Result<Option<StoredData>, DomainError> {
        if !may_commit() {
            return Err(DomainError::PermissionDenied);
        }
        let board_id = prepared.instance.board_id;
        let tx = self.connection.unchecked_transaction().map_err(internal)?;
        let mut current = super::storage::list::<super::types::PluginInstance>(&tx, "instance")?
            .into_iter()
            .find(|i| i.id == prepared.instance.id)
            .ok_or(DomainError::PermissionDenied)?;
        if !super::manager::same_authorization(&prepared.instance, &current)
            || current.pending_domain.is_some()
        {
            return Err(DomainError::PermissionDenied);
        }
        let role = self.board_role(board_id).map_err(DomainError::repository)?;
        if role == BoardRole::Viewer {
            return Err(DomainError::PermissionDenied);
        }
        if version(self, board_id)? != prepared.version {
            return Err(DomainError::StaleBoardRequest);
        }
        super::changeset::persist_sources(&tx, &prepared.instance, &prepared.sources)?;
        if let Some(cursor) = &prepared.cursor {
            super::storage::put(&tx, "cursor", &prepared.instance.id, cursor)?;
        }
        let mut committed = None;
        let mut committed_positions = None;
        if prepared.has_board_operations {
            let before = self
                .read_board_complete(board_id)
                .map_err(DomainError::repository)?;
            let mut after = before.clone();
            after.columns = prepared.after.columns.clone();
            after.label_recency = prepared.after.label_recency.clone();
            // Archives, templates, settings and metadata remain the current values.
            let mut positions = self
                .load_board_positions(board_id)
                .map_err(DomainError::repository)?;
            crate::storage::persist_board_diff_transaction(
                &tx,
                board_id,
                &before,
                &after,
                &mut positions,
                true,
            )
            .map_err(DomainError::repository)?;
            if before != after {
                crate::storage::mark_local_board_change(&tx, board_id, role)
                    .map_err(DomainError::repository)?;
                crate::storage::persist_loro_local_delta(
                    &tx,
                    board_id,
                    &before,
                    &after,
                    self.loro_peers.entry(board_id).or_default(),
                )
                .map_err(DomainError::repository)?;
            }
            committed_positions = Some(positions);
            committed = Some(after);
        }
        let mut run = super::storage::list::<super::types::PluginRun>(&tx, "run")?
            .into_iter()
            .find(|r| r.id == prepared.run_id)
            .ok_or(DomainError::InvalidArgument)?;
        run.status = "success".into();
        run.finished_at = Some(super::types::now());
        run.created = prepared.created;
        run.updated = prepared.updated;
        current.last_run_status = Some("success".into());
        current.last_run_at = run.finished_at;
        current.failures = 0;
        current.last_error = None;
        super::storage::put(&tx, "instance", &current.id, &current)?;
        super::storage::put(&tx, "run", &run.id, &run)?;
        if !may_commit() {
            return Err(DomainError::PermissionDenied);
        }
        tx.commit().map_err(internal)?;
        if self.active_board_id() == board_id {
            if let Some(positions) = committed_positions {
                self.positions = positions;
            }
        }
        Ok(committed)
    }
}
