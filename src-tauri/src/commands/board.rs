use crate::errors::{CommandError, DomainError};
use crate::{
    models::{Column, ColumnSort, StoredData, Task},
    state::{undo_last_change_for_board, update_stored_for_board, SharedAppData},
};
use tauri::{Manager, State};

fn column_index(data: &StoredData, column_id: i64) -> Result<usize, DomainError> {
    data.columns
        .iter()
        .position(|column| column.id == column_id)
        .ok_or(DomainError::ColumnNotFound)
}

fn task_position(data: &StoredData, task_id: i64) -> Result<(usize, usize), DomainError> {
    data.columns
        .iter()
        .enumerate()
        .find_map(|(column_idx, column)| {
            column
                .tasks
                .iter()
                .position(|task| task.id == task_id)
                .map(|task_idx| (column_idx, task_idx))
        })
        .ok_or(DomainError::TaskNotFound)
}

#[tauri::command]
pub fn get_columns(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<Column>, CommandError> {
    let guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    Ok(guard.stored.columns.clone())
}

#[tauri::command]
pub fn get_board_columns(
    state: State<'_, SharedAppData>,
    board_id: i64,
) -> Result<Vec<Column>, CommandError> {
    let guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    if !guard
        .database
        .board_exists(board_id)
        .map_err(CommandError::repository)?
    {
        return Err(CommandError::BoardNotFound);
    }
    Ok(guard
        .database
        .read_board(board_id)
        .map_err(CommandError::repository)?
        .columns)
}

#[tauri::command]
pub async fn search_tasks(
    app: tauri::AppHandle,
    query: String,
    expected_board_id: i64,
    options: Option<crate::search::SearchOptions>,
) -> Result<Vec<i64>, CommandError> {
    if options.is_some_and(|options| !options.valid()) {
        return Err(CommandError::InvalidArgument);
    }
    let path = {
        let state = app.state::<SharedAppData>();
        let guard = state
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
        if guard.active_board_id != expected_board_id {
            return Err(CommandError::StaleBoardRequest);
        }
        guard.database.path().to_owned()
    };
    let ids = tauri::async_runtime::spawn_blocking(move || {
        let reader =
            crate::storage::Database::open_search_reader(path).map_err(CommandError::repository)?;
        match options {
            Some(options) => reader.search_tasks_with_options(expected_board_id, &query, options),
            None => reader.search_tasks(expected_board_id, &query),
        }
        .map_err(CommandError::repository)
    })
    .await
    .map_err(CommandError::internal)??;
    let state = app.state::<SharedAppData>();
    let guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    Ok(ids)
}

#[tauri::command]
pub fn get_labels(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<String>, CommandError> {
    let guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    Ok(guard.ordered_labels())
}

#[tauri::command]
pub fn undo(state: State<'_, SharedAppData>, expected_board_id: i64) -> Result<bool, CommandError> {
    undo_last_change_for_board(&state, expected_board_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn add_column(
    state: State<'_, SharedAppData>,
    name: String,
    color: String,
    expected_board_id: i64,
) -> Result<i64, CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let id = data.allocate_column_id()?;
        data.columns.push(Column {
            id,
            name,
            color,
            sort_order: ColumnSort::Custom,
            tasks: Vec::new(),
        });
        Ok(id)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn update_column(
    state: State<'_, SharedAppData>,
    column_id: i64,
    name: String,
    color: String,
    sort_order: ColumnSort,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let index = column_index(data, column_id)?;
        data.columns[index].name = name;
        data.columns[index].color = color;
        data.columns[index].sort_order = sort_order;
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn delete_column(
    state: State<'_, SharedAppData>,
    column_id: i64,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let index = column_index(data, column_id)?;
        data.columns.remove(index);
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn move_column(
    state: State<'_, SharedAppData>,
    column_id: i64,
    before_column_id: Option<i64>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        if before_column_id == Some(column_id) {
            return Ok(());
        }
        let from = column_index(data, column_id)?;
        let column = data.columns.remove(from);
        let destination = match before_column_id {
            Some(id) => column_index(data, id)?,
            None => data.columns.len(),
        };
        data.columns.insert(destination, column);
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn move_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    to_column_id: i64,
    before_task_id: Option<i64>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        if before_task_id == Some(task_id) {
            return Ok(());
        }
        let (from_column, from_task) = task_position(data, task_id)?;
        let task = data.columns[from_column].tasks.remove(from_task);
        let to_column = column_index(data, to_column_id)?;
        let destination = match before_task_id {
            Some(id) => data.columns[to_column]
                .tasks
                .iter()
                .position(|candidate| candidate.id == id)
                .ok_or(DomainError::TaskNotFound)?,
            None => data.columns[to_column].tasks.len(),
        };
        data.columns[to_column].tasks.insert(destination, task);
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn add_task(
    state: State<'_, SharedAppData>,
    column_id: i64,
    mut task: Task,
    after_task_id: Option<i64>,
    expected_board_id: i64,
) -> Result<i64, CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let column = column_index(data, column_id)?;
        let destination = match after_task_id {
            Some(id) => {
                data.columns[column]
                    .tasks
                    .iter()
                    .position(|candidate| candidate.id == id)
                    .ok_or(DomainError::TaskNotFound)?
                    + 1
            }
            None => data.columns[column].tasks.len(),
        };
        let id = data.allocate_task_id()?;
        task.id = id;
        for label in &task.labels {
            data.touch_label(label.clone());
        }
        data.columns[column].tasks.insert(destination, task);
        Ok(id)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn move_task_to_board(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_id: i64,
    target_board_id: i64,
    target_column_id: i64,
) -> Result<(), CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    let mut source = guard
        .database
        .move_task_to_board(
            expected_board_id,
            task_id,
            target_board_id,
            target_column_id,
        )
        .map_err(CommandError::repository)?;
    if !guard.archives_loaded {
        source.archives.clear();
    }
    guard.stored = source;
    // A single-board undo would duplicate the task left in the destination.
    guard.undo_history.clear();
    guard.refresh_labels();
    guard.boards = guard.database.boards().map_err(CommandError::repository)?;
    Ok(())
}

/// Quick Add deliberately targets an explicit board and never changes global
/// active-board state as a side effect.
#[tauri::command]
pub fn add_task_to_board(
    state: State<'_, SharedAppData>,
    board_id: i64,
    column_id: i64,
    task: Task,
) -> Result<i64, CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    add_task_to_board_locked(&mut guard, board_id, column_id, task)
}

pub(crate) fn add_task_to_board_locked(
    guard: &mut crate::state::AppData,
    board_id: i64,
    column_id: i64,
    mut task: Task,
) -> Result<i64, CommandError> {
    if !guard
        .database
        .board_exists(board_id)
        .map_err(CommandError::repository)?
    {
        return Err(CommandError::BoardNotFound);
    }
    let mut data = guard
        .database
        .read_board_complete(board_id)
        .map_err(CommandError::repository)?;
    let column = column_index(&data, column_id)?;
    let id = data.allocate_task_id()?;
    task.id = id;
    for label in &task.labels {
        data.touch_label(label.clone());
    }
    data.columns[column].tasks.push(task);
    let (revision, status) = guard
        .database
        .replace_board_as_local_edit(board_id, &data)
        .map_err(CommandError::repository)?;
    if board_id == guard.active_board_id {
        // Preserve the lazily-loaded archive contract for the active in-memory
        // view while the complete database write keeps archives intact.
        if !guard.archives_loaded {
            data.archives.clear();
        }
        let previous = std::mem::replace(&mut guard.stored, data);
        guard.record_board_undo(previous);
        guard.refresh_labels();
    }
    if let Some(board) = guard.boards.iter_mut().find(|board| board.id == board_id) {
        board.task_count += 1;
        board.sync_revision = revision;
        board.sync_status = status;
    }
    Ok(id)
}

#[tauri::command]
pub fn delete_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let (column, task) = task_position(data, task_id)?;
        data.columns[column].tasks.remove(task);
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn update_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    mut task: Task,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let (column, index) = task_position(data, task_id)?;
        let previous_labels = data.columns[column].tasks[index].labels.clone();
        task.id = task_id;
        for label in &task.labels {
            if !previous_labels.contains(label) {
                data.touch_label(label.clone());
            }
        }
        data.columns[column].tasks[index] = task;
        Ok(())
    })
    .map_err(CommandError::from)
}
