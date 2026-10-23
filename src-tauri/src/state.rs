use crate::errors::{CommandError, DomainError, ShareError};
use crate::{
    models::{Board, BoardRole, StoredData},
    storage::{self, Database},
};
use std::{
    collections::{BTreeSet, HashSet},
    fmt,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};
use tauri::State;

pub struct AppData {
    pub stored: StoredData,
    pub boards: Vec<Board>,
    pub active_board_id: i64,
    pub undo_history: Vec<StoredData>,
    pub labels: BTreeSet<String>,
    pub database: Database,
    pub recovery_messages: Vec<String>,
    pub archives_loaded: bool,
}

pub type SharedAppData = Mutex<AppData>;

/// A panic while holding the application state left it possibly inconsistent.
#[derive(Debug)]
pub struct AppLockPoisoned;

impl fmt::Display for AppLockPoisoned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Application state lock is poisoned")
    }
}

impl std::error::Error for AppLockPoisoned {}

impl From<AppLockPoisoned> for String {
    fn from(error: AppLockPoisoned) -> Self {
        error.to_string()
    }
}

impl From<AppLockPoisoned> for DomainError {
    fn from(error: AppLockPoisoned) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<AppLockPoisoned> for ShareError {
    fn from(error: AppLockPoisoned) -> Self {
        Self::Internal(error.to_string())
    }
}

impl From<AppLockPoisoned> for CommandError {
    fn from(error: AppLockPoisoned) -> Self {
        Self::internal(error)
    }
}

pub fn lock_app(state: &SharedAppData) -> Result<MutexGuard<'_, AppData>, AppLockPoisoned> {
    state.lock().map_err(|_| AppLockPoisoned)
}

impl AppData {
    pub fn new(
        stored: StoredData,
        database: Database,
        recovery_messages: Vec<String>,
        archives_loaded: bool,
    ) -> Result<Self, String> {
        let boards = database.boards().map_err(|error| error.to_string())?;
        let active_board_id = database.active_board_id();
        let mut data = Self {
            stored,
            boards,
            active_board_id,
            undo_history: Vec::new(),
            labels: BTreeSet::new(),
            database,
            recovery_messages,
            archives_loaded,
        };
        data.refresh_labels();
        data.forget_deleted_muted_columns();
        Ok(data)
    }

    /// Forget muted columns that were deleted here or by other members of a
    /// shared board. Deleting a column cannot do this itself, because undoing
    /// the deletion keeps the current settings; at startup no undo history
    /// can restore a column.
    fn forget_deleted_muted_columns(&mut self) {
        if self.stored.settings.notify_muted_columns.is_empty() {
            return;
        }
        let columns = match self.database.column_ids() {
            Ok(columns) => columns,
            Err(error) => {
                log::warn!(target: "notifications", "Could not read columns to forget deleted muted columns: {error}");
                return;
            }
        };
        let mut muted = self.stored.settings.notify_muted_columns.clone();
        muted.retain(|board_id, column_ids| {
            column_ids.retain(|column_id| columns.contains(&(*board_id, *column_id)));
            !column_ids.is_empty()
        });
        if muted == self.stored.settings.notify_muted_columns {
            return;
        }
        if let Err(error) = update_locked(self, |data| {
            data.settings.notify_muted_columns = muted;
            Ok(())
        }) {
            log::warn!(target: "notifications", "Could not forget deleted muted columns: {error}");
        }
    }

    /// Install board contents that changed outside the local undo history.
    /// Undo snapshots describe the previous contents, so they are discarded.
    pub fn replace_board_contents(&mut self, stored: StoredData) {
        self.stored = stored;
        self.undo_history.clear();
        self.refresh_labels();
    }

    /// Make `board_id` the active board, reloaded from SQLite with archives.
    pub fn load_active_board(&mut self, board_id: i64) -> storage::StorageResult<()> {
        let stored = self.database.load_board(board_id)?;
        self.active_board_id = board_id;
        self.archives_loaded = true;
        self.replace_board_contents(stored);
        Ok(())
    }

    pub fn refresh_labels(&mut self) {
        self.labels = self
            .stored
            .columns
            .iter()
            .flat_map(|column| column.tasks.iter())
            .flat_map(|task| task.labels.iter().cloned())
            .collect();
    }

    pub fn ordered_labels(&self) -> Vec<String> {
        let mut labels = self
            .stored
            .label_recency
            .iter()
            .filter(|label| self.labels.contains(*label))
            .cloned()
            .collect::<Vec<_>>();
        let ordered = labels.iter().cloned().collect::<HashSet<_>>();
        labels.extend(
            self.labels
                .iter()
                .filter(|label| !ordered.contains(label.as_str()))
                .cloned(),
        );
        labels
    }
}

pub fn update_stored<R>(
    state: &State<'_, SharedAppData>,
    change: impl FnOnce(&mut StoredData) -> Result<R, String>,
) -> Result<R, String> {
    let mut guard = lock_app(state)?;
    update_locked(&mut guard, change)
}

/// Apply a board-owned mutation only when the caller's board context is still
/// current.  This prevents delayed UI requests from being written to a board
/// selected after the request was created.
pub fn update_stored_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, DomainError>,
) -> Result<R, DomainError> {
    let mut guard = lock_app(state)?;
    if guard.active_board_id != expected_board_id {
        return Err(DomainError::StaleBoardRequest);
    }
    ensure_board_can_edit(&guard)?;
    update_locked_typed(&mut guard, change)
}

pub fn update_stored_with_archives_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, DomainError>,
) -> Result<R, DomainError> {
    let mut guard = lock_app(state)?;
    if guard.active_board_id != expected_board_id {
        return Err(DomainError::StaleBoardRequest);
    }
    ensure_board_can_edit(&guard)?;
    ensure_archives_loaded_locked(&mut guard)?;
    update_locked_typed(&mut guard, change)
}

pub fn load_archives_for_board(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<crate::models::Archive>, DomainError> {
    let mut guard = lock_app(state)?;
    if guard.active_board_id != expected_board_id {
        return Err(DomainError::StaleBoardRequest);
    }
    ensure_archives_loaded_locked(&mut guard)?;
    Ok(guard.stored.archives.clone())
}

pub fn read_with_archives_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    read: impl FnOnce(&AppData) -> R,
) -> Result<R, DomainError> {
    let mut guard = lock_app(state)?;
    if guard.active_board_id != expected_board_id {
        return Err(DomainError::StaleBoardRequest);
    }
    ensure_archives_loaded_locked(&mut guard)?;
    Ok(read(&guard))
}

pub fn update_stored_with_pre_import_snapshot_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, DomainError>,
) -> Result<(R, PathBuf), DomainError> {
    let mut guard = lock_app(state)?;
    if guard.active_board_id != expected_board_id {
        return Err(DomainError::StaleBoardRequest);
    }
    ensure_board_can_edit(&guard)?;
    ensure_archives_loaded_locked(&mut guard)?;
    let snapshot_path = write_pre_import_snapshot(&guard)?;
    let result = update_locked_typed(&mut guard, change)?;
    Ok((result, snapshot_path))
}

fn write_pre_import_snapshot(guard: &AppData) -> Result<PathBuf, String> {
    let snapshot_path = guard.database.path().with_file_name("data.pre-import.json");
    storage::write_document(&snapshot_path, &guard.stored)
        .map_err(|error| format!("Could not create pre-import backup: {error}"))?;
    Ok(snapshot_path)
}

fn update_locked<R>(
    guard: &mut AppData,
    change: impl FnOnce(&mut StoredData) -> Result<R, String>,
) -> Result<R, String> {
    update_locked_typed(guard, change)
}

fn update_locked_typed<R, E: From<String>>(
    guard: &mut AppData,
    change: impl FnOnce(&mut StoredData) -> Result<R, E>,
) -> Result<R, E> {
    let mut candidate = guard.stored.clone();
    let result = change(&mut candidate)?;
    candidate.keep_pinned_tasks_first();
    let board_changed = candidate.columns != guard.stored.columns
        || candidate.archives != guard.stored.archives
        || candidate.templates != guard.stored.templates;

    if guard.archives_loaded {
        guard
            .database
            .persist_diff(&guard.stored, &candidate)
            .map_err(|error| error.to_string())?;
    } else {
        guard
            .database
            .persist_diff_without_archives(&guard.stored, &candidate)
            .map_err(|error| error.to_string())?;
    }

    let previous = std::mem::replace(&mut guard.stored, candidate);
    refresh_active_board_summary(guard)?;
    if board_changed {
        guard.record_board_undo(previous);
    }
    guard.refresh_labels();
    Ok(result)
}

impl AppData {
    pub(crate) fn record_board_undo(&mut self, previous: StoredData) {
        self.undo_history.push(previous);
        if self.undo_history.len() > 50 {
            self.undo_history.remove(0);
        }
    }
}

fn refresh_active_board_summary(guard: &mut AppData) -> Result<(), String> {
    let (revision, status) = guard
        .database
        .board_sync_state(guard.active_board_id)
        .map_err(|e| e.to_string())?;
    if let Some(board) = guard
        .boards
        .iter_mut()
        .find(|board| board.id == guard.active_board_id)
    {
        board.sync_revision = revision;
        board.sync_status = status;
        board.task_count = guard
            .stored
            .columns
            .iter()
            .map(|column| column.tasks.len() as i64)
            .sum();
    }
    Ok(())
}

// Backend enforcement matters: a client must not bypass the disabled UI by
// directly invoking a mutation command for a received read-only board.
fn ensure_board_can_edit(guard: &AppData) -> Result<(), DomainError> {
    if guard
        .database
        .board_role(guard.active_board_id)
        .map_err(DomainError::repository)?
        == BoardRole::Viewer
    {
        Err(DomainError::PermissionDenied)
    } else {
        Ok(())
    }
}

fn ensure_archives_loaded_locked(guard: &mut AppData) -> Result<(), String> {
    if guard.archives_loaded {
        return Ok(());
    }
    let archives = guard
        .database
        .load_archives()
        .map_err(|error| error.to_string())?;
    guard.stored.archives = archives.clone();
    for snapshot in &mut guard.undo_history {
        snapshot.archives = archives.clone();
    }
    guard.archives_loaded = true;
    Ok(())
}

pub fn undo_last_change_for_board(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<bool, DomainError> {
    let mut guard = lock_app(state)?;
    undo_locked(&mut guard, expected_board_id)
}

fn undo_locked(guard: &mut AppData, expected_board_id: i64) -> Result<bool, DomainError> {
    if guard.active_board_id != expected_board_id {
        return Err(DomainError::StaleBoardRequest);
    }
    ensure_board_can_edit(guard)?;
    let mut previous = guard
        .undo_history
        .last()
        .cloned()
        .ok_or(DomainError::NothingToUndo)?;
    previous.settings = guard.stored.settings.clone();

    let archives_loaded = guard.archives_loaded;
    let AppData {
        database, stored, ..
    } = &mut *guard;
    if archives_loaded {
        database
            .persist_diff(stored, &previous)
            .map_err(|error| error.to_string())?;
    } else {
        database
            .persist_diff_without_archives(stored, &previous)
            .map_err(|error| error.to_string())?;
    }
    guard.undo_history.pop();
    guard.stored = previous;
    refresh_active_board_summary(guard)?;
    guard.refresh_labels();
    Ok(!guard.undo_history.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Archive, Column, Task};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn app_data(dir: &std::path::Path, stored: StoredData) -> AppData {
        let mut database = Database::open(dir.join("data.sqlite3")).unwrap();
        database.initialize_boards(&stored).unwrap();
        AppData::new(stored, database, Vec::new(), true).unwrap()
    }

    #[test]
    fn startup_forgets_muted_columns_that_no_longer_exist() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-muted-columns-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let column = |id| Column {
            id,
            name: format!("Column {id}"),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            tasks: Vec::new(),
        };
        let stored = StoredData {
            columns: vec![column(10), column(11)],
            ..StoredData::default()
        };
        let app = app_data(&dir, stored);
        let board_id = app.active_board_id;
        drop(app);

        let mut database = Database::open(dir.join("data.sqlite3")).unwrap();
        let before = database.load_board(board_id).unwrap();
        let mut stored = before.clone();
        stored.settings.notify_muted_columns = std::collections::BTreeMap::from([
            (board_id, BTreeSet::from([10, 99])),
            (board_id + 1, BTreeSet::from([10])),
        ]);
        database.persist_diff(&before, &stored).unwrap();
        let app = AppData::new(stored, database, Vec::new(), true).unwrap();

        let expected = std::collections::BTreeMap::from([(board_id, BTreeSet::from([10]))]);
        assert_eq!(app.stored.settings.notify_muted_columns, expected);
        assert!(app.undo_history.is_empty());
        assert_eq!(
            app.database
                .read_board(board_id)
                .unwrap()
                .settings
                .notify_muted_columns,
            expected
        );
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_change_does_not_commit_memory_candidate() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-failed-change-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut app = app_data(&dir, StoredData::default());

        let result: Result<(), String> = update_locked(&mut app, |data| {
            data.columns.push(Column {
                id: 0,
                name: "Must not commit".into(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                tasks: Vec::new(),
            });
            Err("Rejected change".to_string())
        });

        assert!(result.is_err());
        assert!(app.stored.columns.is_empty());
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pre_import_snapshot_survives_later_regular_writes() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-pre-import-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let mut stored = StoredData::default();
        stored.columns.push(Column {
            id: 0,
            name: "Before import".into(),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            tasks: Vec::new(),
        });
        let mut app = app_data(&dir, stored);
        let snapshot_path = write_pre_import_snapshot(&app).unwrap();

        update_locked(&mut app, |data| {
            data.columns[0].name = "Imported".into();
            Ok(())
        })
        .unwrap();
        update_locked(&mut app, |data| {
            data.columns[0].name = "Edited after import".into();
            Ok(())
        })
        .unwrap();

        let snapshot: StoredData =
            serde_json::from_str(&fs::read_to_string(snapshot_path).unwrap()).unwrap();
        assert_eq!(snapshot.columns[0].name, "Before import");
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_board_column_creation_preserves_active_board_and_undo() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-column-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut app = app_data(&dir, StoredData::default());
        let active = app.active_board_id;
        let target = app.database.create_board("Target").unwrap().id;
        let id =
            crate::commands::board::add_column_to_board_locked(&mut app, target, " Todo ".into())
                .unwrap();
        assert_eq!(app.active_board_id, active);
        assert!(app.stored.columns.is_empty());
        assert!(app.undo_history.is_empty());
        let data = app.database.read_board_complete(target).unwrap();
        assert_eq!(data.columns[0].id, id);
        assert_eq!(data.columns[0].name, "Todo");
        assert!(
            crate::commands::board::add_column_to_board_locked(&mut app, target, " ".into())
                .is_err()
        );
        crate::commands::board::add_column_to_board_locked(&mut app, active, "Active".into())
            .unwrap();
        assert!(!undo_locked(&mut app, active).unwrap());
        assert!(app.stored.columns.is_empty());
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn successful_updates_are_kept_for_undo() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-undo-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut app = app_data(&dir, StoredData::default());

        update_locked(&mut app, |data| {
            data.columns.push(Column {
                id: 0,
                name: "Added".into(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                tasks: Vec::new(),
            });
            Ok(())
        })
        .unwrap();

        assert_eq!(app.undo_history.len(), 1);
        assert!(app.undo_history[0].columns.is_empty());
        update_locked(&mut app, |data| {
            data.settings.notify_enabled = !data.settings.notify_enabled;
            data.settings.theme = Some("morandi-dark-blue".into());
            data.settings.iroh_network.listen_port = 12345;
            Ok(())
        })
        .unwrap();
        assert_eq!(app.undo_history.len(), 1);
        let settings = app.stored.settings.clone();
        let board_id = app.active_board_id;
        crate::commands::board::add_task_to_board_locked(&mut app, board_id, 0, Task::default())
            .unwrap();
        assert_eq!(app.undo_history.len(), 2);
        assert_eq!(app.stored.columns[0].tasks.len(), 1);
        assert!(undo_locked(&mut app, board_id).unwrap());
        assert_eq!(app.stored.columns.len(), 1);
        assert!(app.stored.columns[0].tasks.is_empty());
        assert!(!undo_locked(&mut app, board_id).unwrap());
        assert!(app.stored.columns.is_empty());
        assert_eq!(app.stored.settings, settings);
        assert_eq!(
            app.database.read_board_complete(board_id).unwrap().settings,
            settings
        );
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loading_archives_hydrates_existing_undo_snapshots() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-lazy-undo-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let archive = Archive {
            time: 123,
            task: Task {
                id: 7,
                title: "Archived".into(),
                ..Task::default()
            },
        };
        let mut persisted = StoredData::default();
        persisted.archives.push(archive.clone());
        persisted.next_task_id = 8;
        let mut database = Database::open(dir.join("data.sqlite3")).unwrap();
        database.initialize_boards(&persisted).unwrap();

        let mut unloaded = persisted;
        unloaded.archives.clear();
        let mut app = AppData::new(unloaded, database, Vec::new(), false).unwrap();
        update_locked(&mut app, |data| {
            data.columns.push(Column {
                id: 0,
                name: "Added before opening archives".into(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                tasks: Vec::new(),
            });
            Ok(())
        })
        .unwrap();

        ensure_archives_loaded_locked(&mut app).unwrap();
        assert_eq!(app.stored.archives, vec![archive.clone()]);
        assert_eq!(app.undo_history[0].archives, vec![archive]);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reloading_the_active_board_discards_undo_and_refreshes_derived_state() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-reload-board-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut persisted = StoredData::default();
        persisted.columns.push(Column {
            id: 1,
            name: "Persisted".into(),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            tasks: vec![Task {
                id: 1,
                title: "Labelled".into(),
                labels: vec!["saved".into()],
                ..Task::default()
            }],
        });
        persisted.archives.push(Archive {
            time: 123,
            task: Task {
                id: 2,
                title: "Archived".into(),
                ..Task::default()
            },
        });
        persisted.next_task_id = 3;
        let mut database = Database::open(dir.join("data.sqlite3")).unwrap();
        database.initialize_boards(&persisted).unwrap();
        let board_id = database.active_board_id();
        let mut app = AppData::new(StoredData::default(), database, Vec::new(), false).unwrap();
        app.undo_history.push(StoredData::default());

        app.load_active_board(board_id).unwrap();

        assert_eq!(app.active_board_id, board_id);
        assert!(app.archives_loaded);
        assert_eq!(app.stored.columns, persisted.columns);
        assert_eq!(app.stored.archives, persisted.archives);
        assert!(app.undo_history.is_empty());
        assert_eq!(app.ordered_labels(), vec!["saved".to_string()]);
        drop(app);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn poisoned_app_lock_keeps_the_internal_error_for_every_boundary() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-poisoned-lock-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let state: SharedAppData = Mutex::new(app_data(&dir, StoredData::default()));
        assert!(lock_app(&state).is_ok());
        std::thread::scope(|scope| {
            let _ = scope
                .spawn(|| {
                    let _guard = state.lock().unwrap();
                    panic!("poison the application state");
                })
                .join();
        });

        let message = "Application state lock is poisoned";
        let poisoned = || lock_app(&state).err().unwrap();
        assert_eq!(String::from(poisoned()), message);
        assert!(matches!(
            DomainError::from(poisoned()),
            DomainError::Internal(detail) if detail == message
        ));
        assert!(matches!(
            ShareError::from(poisoned()),
            ShareError::Internal(detail) if detail == message
        ));
        assert!(matches!(
            CommandError::from(poisoned()),
            CommandError::InternalError
        ));
        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }
}
