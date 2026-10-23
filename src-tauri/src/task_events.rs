//! Device-local, append-only log of task lifecycle events.
//!
//! Each board write that changes where tasks are records one operation with
//! the events it caused. Rows are never updated or deleted (except with their
//! board), so ids only grow and a reader such as a future plugin can resume
//! from the last id it handled. Undo appends an operation that names the one
//! it reverts instead of removing history. Events hold ids only: readers load
//! current task content through normal board access. The log is not part of
//! board content, synchronization, exports, or Undo.

use crate::models::StoredData;
use rusqlite::{params, Connection, Transaction};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

type EventResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Who caused an operation. Stored as stable strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOrigin {
    Local,
    Remote,
    Plugin,
    Undo,
}

impl EventOrigin {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
            Self::Plugin => "plugin",
            Self::Undo => "undo",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "local" => Self::Local,
            "remote" => Self::Remote,
            "plugin" => Self::Plugin,
            "undo" => Self::Undo,
            _ => return None,
        })
    }
}

/// Stored as stable strings without a database constraint, so later versions
/// can add kinds; readers skip kinds they do not know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Created,
    Moved,
    Archived,
    Restored,
    Deleted,
    TransferredIn,
    TransferredOut,
    /// Task ids before this event no longer identify the same tasks.
    BoardReset,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Moved => "moved",
            Self::Archived => "archived",
            Self::Restored => "restored",
            Self::Deleted => "deleted",
            Self::TransferredIn => "transferred_in",
            Self::TransferredOut => "transferred_out",
            Self::BoardReset => "board_reset",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "created" => Self::Created,
            "moved" => Self::Moved,
            "archived" => Self::Archived,
            "restored" => Self::Restored,
            "deleted" => Self::Deleted,
            "transferred_in" => Self::TransferredIn,
            "transferred_out" => Self::TransferredOut,
            "board_reset" => Self::BoardReset,
            _ => return None,
        })
    }
}

/// How the events of one write are attributed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventSource {
    origin: EventOrigin,
    plugin_instance_id: Option<String>,
    reverts_op_id: Option<i64>,
}

impl EventSource {
    pub fn local() -> Self {
        Self::new(EventOrigin::Local)
    }

    pub fn remote() -> Self {
        Self::new(EventOrigin::Remote)
    }

    pub fn plugin(instance_id: &str) -> Self {
        Self {
            plugin_instance_id: Some(instance_id.to_owned()),
            ..Self::new(EventOrigin::Plugin)
        }
    }

    /// `reverts` is the operation recorded for the undone change, if any.
    pub fn undo(reverts: Option<i64>) -> Self {
        Self {
            reverts_op_id: reverts,
            ..Self::new(EventOrigin::Undo)
        }
    }

    fn new(origin: EventOrigin) -> Self {
        Self {
            origin,
            plugin_instance_id: None,
            reverts_op_id: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    task_id: i64,
    kind: EventKind,
    from_column_id: Option<i64>,
    to_column_id: Option<i64>,
}

/// One stored event joined with its operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskEvent {
    pub id: i64,
    pub op_id: i64,
    pub origin: EventOrigin,
    pub plugin_instance_id: Option<String>,
    pub reverts_op_id: Option<i64>,
    pub at: i64,
    pub task_id: i64,
    pub kind: EventKind,
    pub from_column_id: Option<i64>,
    pub to_column_id: Option<i64>,
}

pub fn initialize(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS cardbe_task_event_ops(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,
            origin TEXT NOT NULL, plugin_instance_id TEXT,
            reverts_op_id INTEGER, at INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS cardbe_task_event_ops_by_board
            ON cardbe_task_event_ops(board_id,id);
         CREATE TABLE IF NOT EXISTS cardbe_task_events(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            op_id INTEGER NOT NULL REFERENCES cardbe_task_event_ops(id) ON DELETE CASCADE,
            board_id INTEGER NOT NULL, task_id INTEGER NOT NULL, kind TEXT NOT NULL,
            from_column_id INTEGER, to_column_id INTEGER);
         CREATE INDEX IF NOT EXISTS cardbe_task_events_by_op ON cardbe_task_events(op_id);
         CREATE INDEX IF NOT EXISTS cardbe_task_events_by_board_task
            ON cardbe_task_events(board_id,task_id,id);
         -- Unreleased device-local done columns, replaced by the synced column flag.
         DROP TABLE IF EXISTS cardbe_done_columns;",
    )
}

/// Records how tasks moved between `before` and `after`. Returns the new
/// operation id, or `None` when nothing was recorded.
pub fn record(
    tx: &Transaction<'_>,
    board_id: i64,
    before: &StoredData,
    after: &StoredData,
    source: &EventSource,
) -> EventResult<Option<i64>> {
    insert(tx, board_id, source, &changes(before, after, false))
}

/// Like [`record`] for a move between boards: tasks that leave or arrive are
/// transfers rather than deletions or creations.
pub fn record_transfer(
    tx: &Transaction<'_>,
    board_id: i64,
    before: &StoredData,
    after: &StoredData,
) -> EventResult<Option<i64>> {
    insert(
        tx,
        board_id,
        &EventSource::local(),
        &changes(before, after, true),
    )
}

/// Marks that the board's task ids were replaced, for example by an import.
pub fn record_reset(
    tx: &Transaction<'_>,
    board_id: i64,
    source: &EventSource,
) -> EventResult<Option<i64>> {
    let reset = Change {
        task_id: 0,
        kind: EventKind::BoardReset,
        from_column_id: None,
        to_column_id: None,
    };
    insert(tx, board_id, source, &[reset])
}

/// Events of a board in recording order, optionally only those of one task
/// plus board resets, and only those recorded after `after_id`. Unknown
/// stored kinds are skipped.
pub fn list(
    connection: &Connection,
    board_id: i64,
    task_id: Option<i64>,
    after_id: Option<i64>,
) -> EventResult<Vec<TaskEvent>> {
    let mut statement = connection.prepare(
        "SELECT e.id,e.op_id,o.origin,o.plugin_instance_id,o.reverts_op_id,o.at,
                e.task_id,e.kind,e.from_column_id,e.to_column_id
         FROM cardbe_task_events e JOIN cardbe_task_event_ops o ON o.id=e.op_id
         WHERE e.board_id=?1 AND (?2 IS NULL OR e.task_id=?2 OR e.kind='board_reset')
           AND (?3 IS NULL OR e.id>?3)
         ORDER BY e.id",
    )?;
    let rows = statement
        .query_map(params![board_id, task_id, after_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<i64>>(9)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(
            |(
                id,
                op_id,
                origin,
                plugin_instance_id,
                reverts_op_id,
                at,
                task_id,
                kind,
                from,
                to,
            )| {
                Some(TaskEvent {
                    id,
                    op_id,
                    origin: EventOrigin::parse(&origin)?,
                    plugin_instance_id,
                    reverts_op_id,
                    at,
                    task_id,
                    kind: EventKind::parse(&kind)?,
                    from_column_id: from,
                    to_column_id: to,
                })
            },
        )
        .collect())
}

fn insert(
    tx: &Transaction<'_>,
    board_id: i64,
    source: &EventSource,
    changes: &[Change],
) -> EventResult<Option<i64>> {
    // An undo is still recorded without events so readers see the revert.
    if changes.is_empty() && source.reverts_op_id.is_none() {
        return Ok(None);
    }
    tx.execute(
        "INSERT INTO cardbe_task_event_ops(board_id,origin,plugin_instance_id,reverts_op_id,at)
         VALUES(?1,?2,?3,?4,?5)",
        params![
            board_id,
            source.origin.as_str(),
            source.plugin_instance_id,
            source.reverts_op_id,
            now_millis()?,
        ],
    )?;
    let op_id = tx.last_insert_rowid();
    let mut statement = tx.prepare_cached(
        "INSERT INTO cardbe_task_events(op_id,board_id,task_id,kind,from_column_id,to_column_id)
         VALUES(?1,?2,?3,?4,?5,?6)",
    )?;
    for change in changes {
        statement.execute(params![
            op_id,
            board_id,
            change.task_id,
            change.kind.as_str(),
            change.from_column_id,
            change.to_column_id,
        ])?;
    }
    Ok(Some(op_id))
}

fn now_millis() -> EventResult<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

/// Task placement changes in board order: arrivals and moves first, then
/// departures, then archived tasks deleted outright.
fn changes(before: &StoredData, after: &StoredData, transfer: bool) -> Vec<Change> {
    let placed = |data: &StoredData| {
        data.columns
            .iter()
            .flat_map(|column| column.tasks.iter().map(move |task| (task.id, column.id)))
            .collect::<HashMap<_, _>>()
    };
    let archived = |data: &StoredData| {
        data.archives
            .iter()
            .map(|archive| archive.task.id)
            .collect::<HashSet<_>>()
    };
    let (placed_before, placed_after) = (placed(before), placed(after));
    let (archived_before, archived_after) = (archived(before), archived(after));
    let mut changes = Vec::new();
    for column in &after.columns {
        for task in &column.tasks {
            let (kind, from) = match placed_before.get(&task.id) {
                Some(&from) if from == column.id => continue,
                Some(&from) => (EventKind::Moved, Some(from)),
                None if archived_before.contains(&task.id)
                    && !archived_after.contains(&task.id) =>
                {
                    (EventKind::Restored, None)
                }
                None if transfer => (EventKind::TransferredIn, None),
                None => (EventKind::Created, None),
            };
            changes.push(Change {
                task_id: task.id,
                kind,
                from_column_id: from,
                to_column_id: Some(column.id),
            });
        }
    }
    for column in &before.columns {
        for task in &column.tasks {
            if placed_after.contains_key(&task.id) {
                continue;
            }
            let kind = if archived_after.contains(&task.id) && !archived_before.contains(&task.id) {
                EventKind::Archived
            } else if transfer {
                EventKind::TransferredOut
            } else {
                EventKind::Deleted
            };
            changes.push(Change {
                task_id: task.id,
                kind,
                from_column_id: Some(column.id),
                to_column_id: None,
            });
        }
    }
    for archive in &before.archives {
        let id = archive.task.id;
        if !archived_after.contains(&id) && !placed_after.contains_key(&id) {
            changes.push(Change {
                task_id: id,
                kind: EventKind::Deleted,
                from_column_id: None,
                to_column_id: None,
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Archive, Column, Task};

    fn task(id: i64) -> Task {
        Task {
            id,
            ..Task::default()
        }
    }

    fn board(columns: &[(i64, &[i64])], archives: &[i64]) -> StoredData {
        StoredData {
            columns: columns
                .iter()
                .map(|(id, tasks)| Column {
                    id: *id,
                    name: String::new(),
                    color: String::new(),
                    sort_order: Default::default(),
                    sort_rules: Vec::new(),
                    done: false,
                    tasks: tasks.iter().map(|id| task(*id)).collect(),
                })
                .collect(),
            archives: archives
                .iter()
                .map(|id| Archive {
                    time: 1,
                    task: task(*id),
                })
                .collect(),
            ..StoredData::default()
        }
    }

    fn change(task_id: i64, kind: EventKind, from: Option<i64>, to: Option<i64>) -> Change {
        Change {
            task_id,
            kind,
            from_column_id: from,
            to_column_id: to,
        }
    }

    #[test]
    fn classifies_each_placement_change() {
        let before = board(&[(1, &[10, 11, 12, 13]), (2, &[14])], &[20, 21]);
        // 10 stays, 11 is reordered, 12 moves, 13 is archived, 14 is deleted,
        // 20 is restored, 21 is deleted from the archive, and 30 is new.
        let after = board(&[(1, &[11, 10, 20]), (2, &[12, 30])], &[13]);
        use EventKind::*;
        assert_eq!(
            changes(&before, &after, false),
            vec![
                change(20, Restored, None, Some(1)),
                change(12, Moved, Some(1), Some(2)),
                change(30, Created, None, Some(2)),
                change(13, Archived, Some(1), None),
                change(14, Deleted, Some(2), None),
                change(21, Deleted, None, None),
            ]
        );
        assert!(changes(&after, &after, false).is_empty());
    }

    #[test]
    fn transfers_replace_creation_and_deletion() {
        let before = board(&[(1, &[10, 11])], &[]);
        let after = board(&[(1, &[11, 12])], &[]);
        assert_eq!(
            changes(&before, &after, true),
            vec![
                change(12, EventKind::TransferredIn, None, Some(1)),
                change(10, EventKind::TransferredOut, Some(1), None),
            ]
        );
    }

    #[test]
    fn stored_names_round_trip() {
        use EventKind::*;
        for kind in [
            Created,
            Moved,
            Archived,
            Restored,
            Deleted,
            TransferredIn,
            TransferredOut,
            BoardReset,
        ] {
            assert_eq!(EventKind::parse(kind.as_str()), Some(kind));
        }
        for origin in [
            EventOrigin::Local,
            EventOrigin::Remote,
            EventOrigin::Plugin,
            EventOrigin::Undo,
        ] {
            assert_eq!(EventOrigin::parse(origin.as_str()), Some(origin));
        }
        assert_eq!(EventKind::parse("updated"), None);
    }
}
