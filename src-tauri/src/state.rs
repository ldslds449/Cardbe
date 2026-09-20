use crate::{
    models::{Board, StoredData},
    storage::{self, Database},
};
use std::{
    collections::{BTreeSet, HashSet},
    path::PathBuf,
    sync::Mutex,
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
    pub notified_tasks: HashSet<(i64, i64, u128)>,
    pub archives_loaded: bool,
}

pub type SharedAppData = Mutex<AppData>;

impl AppData {
    pub fn new(
        stored: StoredData,
        database: Database,
        recovery_messages: Vec<String>,
        archives_loaded: bool,
    ) -> Self {
        let boards = database.boards().unwrap_or_default();
        let active_board_id = database.active_board_id();
        let mut data = Self {
            stored,
            boards,
            active_board_id,
            undo_history: Vec::new(),
            labels: BTreeSet::new(),
            database,
            recovery_messages,
            notified_tasks: HashSet::new(),
            archives_loaded,
        };
        data.refresh_labels();
        data
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
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    update_locked(&mut guard, change)
}

/// Apply a board-owned mutation only when the caller's board context is still
/// current.  This prevents delayed UI requests from being written to a board
/// selected after the request was created.
pub fn update_stored_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, String>,
) -> Result<R, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err(format!(
            "Stale board request: expected board {expected_board_id}, active board is {}",
            guard.active_board_id
        ));
    }
    update_locked(&mut guard, change)
}

pub fn update_stored_with_archives_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, String>,
) -> Result<R, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    ensure_archives_loaded_locked(&mut guard)?;
    update_locked(&mut guard, change)
}

pub fn load_archives_for_board(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<crate::models::Archive>, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    ensure_archives_loaded_locked(&mut guard)?;
    Ok(guard.stored.archives.clone())
}

pub fn read_with_archives_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    read: impl FnOnce(&AppData) -> R,
) -> Result<R, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    ensure_archives_loaded_locked(&mut guard)?;
    Ok(read(&guard))
}

pub fn update_stored_with_pre_import_snapshot_for_board<R>(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, String>,
) -> Result<(R, PathBuf), String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    ensure_archives_loaded_locked(&mut guard)?;
    let snapshot_path = write_pre_import_snapshot(&guard)?;
    let result = update_locked(&mut guard, change)?;
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
    let mut candidate = guard.stored.clone();
    let result = change(&mut candidate)?;
    candidate.sort_column_tasks();

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
    guard.undo_history.push(previous);
    // Keep memory usage bounded while still allowing a useful sequence of
    // edits to be recovered.
    if guard.undo_history.len() > 50 {
        guard.undo_history.remove(0);
    }
    guard.refresh_labels();
    Ok(result)
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
) -> Result<bool, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    let previous = guard
        .undo_history
        .last()
        .cloned()
        .ok_or_else(|| "There is nothing to undo".to_string())?;

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
        AppData::new(stored, database, Vec::new(), true)
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
                tasks: Vec::new(),
            });
            Ok(())
        })
        .unwrap();

        assert_eq!(app.undo_history.len(), 1);
        assert!(app.undo_history[0].columns.is_empty());
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
        let mut app = AppData::new(unloaded, database, Vec::new(), false);
        update_locked(&mut app, |data| {
            data.columns.push(Column {
                id: 0,
                name: "Added before opening archives".into(),
                color: String::new(),
                sort_order: Default::default(),
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
}
