use crate::board_mutations::{column_index, task_position};
use crate::errors::CommandError;
use crate::{
    models::{Column, ColumnSort, SortRule, Task},
    state::{
        lock_app, undo_last_change_for_board, update_stored_for_board,
        update_stored_with_archives_for_board, SharedAppData,
    },
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};

/// New tasks record their creation time on the backend so every entry point
/// (dialogs, Quick Add, transfers) is stamped consistently.
fn stamp_created_time(task: &mut Task) {
    if task.created_time == 0 {
        if let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) {
            task.created_time = duration.as_millis();
        }
    }
}

#[tauri::command]
pub fn get_columns(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<Column>, CommandError> {
    let guard = lock_app(&state)?;
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
    let guard = lock_app(&state)?;
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
        let guard = lock_app(&state)?;
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
    let guard = lock_app(&state)?;
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
    let guard = lock_app(&state)?;
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
        crate::board_mutations::add_column(data, name, color)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn update_column(
    state: State<'_, SharedAppData>,
    column_id: i64,
    name: String,
    color: String,
    sort_rules: Vec<SortRule>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::board_mutations::update_column(data, column_id, name, color, sort_rules)
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
        crate::board_mutations::move_column(data, column_id, before_column_id)
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
        crate::board_mutations::move_task(data, task_id, to_column_id, before_task_id)
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
    stamp_created_time(&mut task);
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::board_mutations::add_task(data, column_id, task, after_task_id)
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
    move_tasks_between_boards(
        &state,
        expected_board_id,
        &[task_id],
        target_board_id,
        target_column_id,
    )
}

#[tauri::command]
pub fn move_tasks_to_board(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_ids: Vec<i64>,
    target_board_id: i64,
    target_column_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    move_tasks_between_boards(
        &state,
        expected_board_id,
        &task_ids,
        target_board_id,
        target_column_id,
    )
}

fn move_tasks_between_boards(
    state: &State<'_, SharedAppData>,
    expected_board_id: i64,
    task_ids: &[i64],
    target_board_id: i64,
    target_column_id: i64,
) -> Result<(), CommandError> {
    let mut guard = lock_app(state)?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    let mut source = guard
        .database
        .move_tasks_to_board(
            expected_board_id,
            task_ids,
            target_board_id,
            target_column_id,
        )
        .map_err(CommandError::repository)?;
    if !guard.archives_loaded {
        source.archives.clear();
    }
    // A single-board undo would duplicate the tasks left in the destination.
    guard.replace_board_contents(source);
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
    let mut guard = lock_app(&state)?;
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
    stamp_created_time(&mut task);
    for label in &task.labels {
        data.touch_label(label.clone());
    }
    let before = data.clone();
    data.columns[column].tasks.push(task);
    let now = crate::commands::archive::current_time_millis()?;
    crate::completion::reconcile(&before, &mut data, now)?;
    let ((revision, status), event_op) = guard
        .database
        .replace_board_as_local_edit_recorded(board_id, &data)
        .map_err(CommandError::repository)?;
    if board_id == guard.active_board_id {
        // Preserve the lazily-loaded archive contract for the active in-memory
        // view while the complete database write keeps archives intact.
        if !guard.archives_loaded {
            data.archives.clear();
        }
        let previous = std::mem::replace(&mut guard.stored, data);
        guard.record_board_undo(previous, event_op);
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
pub fn delete_tasks(
    state: State<'_, SharedAppData>,
    task_ids: Vec<i64>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::board_mutations::delete_tasks(data, &task_ids)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn move_tasks(
    state: State<'_, SharedAppData>,
    task_ids: Vec<i64>,
    to_column_id: i64,
    before_task_id: Option<i64>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::board_mutations::move_tasks(data, &task_ids, to_column_id, before_task_id)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_tasks_pinned(
    state: State<'_, SharedAppData>,
    task_ids: Vec<i64>,
    pinned: bool,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::board_mutations::set_tasks_pinned(data, &task_ids, pinned)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_tasks_priority(
    state: State<'_, SharedAppData>,
    task_ids: Vec<i64>,
    priority: Option<crate::models::TaskPriority>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::board_mutations::set_tasks_priority(data, &task_ids, priority)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_done_column(
    state: State<'_, SharedAppData>,
    column_id: Option<i64>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::completion::set_done_column(data, column_id)
    })
    .map_err(CommandError::from)
}

/// Moves tasks to the done column, or archives them when there is none.
#[tauri::command]
pub fn complete_tasks(
    state: State<'_, SharedAppData>,
    task_ids: Vec<i64>,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    let now = crate::commands::archive::current_time_millis()?;
    update_stored_with_archives_for_board(&state, expected_board_id, |data| {
        crate::completion::complete_tasks(data, &task_ids, now)
    })
    .map_err(CommandError::from)
}

/// Moves done or archived tasks to an open column chosen by the user.
#[tauri::command]
pub fn reopen_tasks(
    state: State<'_, SharedAppData>,
    task_ids: Vec<i64>,
    column_id: i64,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    update_stored_with_archives_for_board(&state, expected_board_id, |data| {
        crate::completion::reopen_tasks(data, &task_ids, column_id)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn update_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    task: Task,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        crate::board_mutations::update_task(data, task_id, task)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn add_column_to_board(
    state: State<'_, SharedAppData>,
    board_id: i64,
    name: String,
) -> Result<i64, CommandError> {
    let mut guard = lock_app(&state)?;
    add_column_to_board_locked(&mut guard, board_id, name)
}
pub(crate) fn add_column_to_board_locked(
    guard: &mut crate::state::AppData,
    board_id: i64,
    name: String,
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
    if name.trim().is_empty() {
        return Err(CommandError::InvalidArgument);
    }
    let id = data.allocate_column_id()?;
    data.columns.push(Column {
        id,
        name: name.trim().to_owned(),
        color: String::new(),
        sort_order: ColumnSort::Custom,
        sort_rules: Vec::new(),
        done: false,
        tasks: Vec::new(),
    });
    let ((revision, status), event_op) = guard
        .database
        .replace_board_as_local_edit_recorded(board_id, &data)
        .map_err(CommandError::repository)?;
    if board_id == guard.active_board_id {
        // Preserve the lazily-loaded archive contract for the active in-memory
        // view while the complete database write keeps archives intact.
        if !guard.archives_loaded {
            data.archives.clear();
        }
        let previous = std::mem::replace(&mut guard.stored, data);
        guard.record_board_undo(previous, event_op);
        guard.refresh_labels();
    }
    if let Some(board) = guard.boards.iter_mut().find(|board| board.id == board_id) {
        board.sync_revision = revision;
        board.sync_status = status;
    }
    Ok(id)
}
