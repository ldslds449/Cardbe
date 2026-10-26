use crate::errors::{CommandError, DomainError};
use crate::{
    focuses::{self, Focus},
    models::{StoredData, TaskPriority},
    operations::OperationHost,
    state::{lock_app, update_stored_with_archives_for_board, SharedAppData},
};
use tauri::{AppHandle, Emitter, Manager, State};

/// `None` until views were first saved.
#[tauri::command]
pub fn get_focuses(state: State<'_, SharedAppData>) -> Result<Option<Vec<Focus>>, CommandError> {
    let guard = lock_app(&state)?;
    focuses::load(&guard.database.connection).map_err(CommandError::from)
}

/// Replaces the saved views, in display order.
#[tauri::command]
pub fn set_focuses(
    app: AppHandle,
    state: State<'_, SharedAppData>,
    views: Vec<Focus>,
) -> Result<(), CommandError> {
    {
        let guard = lock_app(&state)?;
        focuses::save(&guard.database.connection, &views).map_err(CommandError::from)?;
    }
    // Floating views follow renames, filter changes, and deletion.
    if let Err(error) = app.emit(FOCUSES_CHANGED_EVENT, ()) {
        log::warn!(target: "focuses", "Could not announce saved Focuses: {error}");
    }
    Ok(())
}

const FOCUSES_CHANGED_EVENT: &str = "cardbe:focuses-changed";

fn ensure_view_exists(state: &State<'_, SharedAppData>, id: &str) -> Result<(), CommandError> {
    if id == focuses::DAILY_PLAN_ID {
        return Ok(());
    }
    let guard = lock_app(state)?;
    let views = focuses::load(&guard.database.connection).map_err(CommandError::from)?;
    views
        .unwrap_or_default()
        .iter()
        .any(|view| view.id == id)
        .then_some(())
        .ok_or(CommandError::FocusNotFound)
}

// Async so that window creation does not deadlock the Windows event loop.
#[tauri::command]
pub async fn open_floating_focus(
    app: AppHandle,
    state: State<'_, SharedAppData>,
    id: String,
) -> Result<(), CommandError> {
    ensure_view_exists(&state, &id)?;
    #[cfg(desktop)]
    crate::desktop::open_floating_focus_window(&app, &id).map_err(CommandError::internal)?;
    #[cfg(not(desktop))]
    let _ = app;
    Ok(())
}

#[tauri::command]
pub async fn open_focus_in_main_window(
    app: AppHandle,
    state: State<'_, SharedAppData>,
    id: String,
) -> Result<(), CommandError> {
    ensure_view_exists(&state, &id)?;
    #[cfg(desktop)]
    crate::desktop::open_focus_in_main_window(&app, &id).map_err(CommandError::internal)?;
    #[cfg(not(desktop))]
    let _ = app;
    Ok(())
}

/// Changes a board from a Focus, which may show any board. Changes to
/// the open board enter its Undo history like other local edits; other boards
/// are written like MCP edits, outside Undo.
fn write_board<R>(
    app: &tauri::AppHandle,
    board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, DomainError>,
) -> Result<R, CommandError> {
    let state = app.state::<SharedAppData>();
    let active = lock_app(&state)?.active_board_id == board_id;
    if !active {
        return crate::operations::board_write::commit(app, board_id, change)
            .map_err(CommandError::from);
    }
    let result = update_stored_with_archives_for_board(&state, board_id, change)?;
    // Other windows showing the board reload it.
    app.board_changed(board_id);
    Ok(result)
}

#[tauri::command]
pub fn complete_board_tasks(
    app: tauri::AppHandle,
    board_id: i64,
    task_ids: Vec<i64>,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    let now = crate::commands::archive::current_time_millis()?;
    write_board(&app, board_id, |data| {
        crate::completion::complete_tasks(data, &task_ids, now)
    })
}

/// Moves done or archived tasks to an open column of their board.
#[tauri::command]
pub fn reopen_board_tasks(
    app: tauri::AppHandle,
    board_id: i64,
    task_ids: Vec<i64>,
    column_id: i64,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    write_board(&app, board_id, |data| {
        crate::completion::reopen_tasks(data, &task_ids, column_id)
    })
}

#[tauri::command]
pub fn set_board_tasks_priority(
    app: tauri::AppHandle,
    board_id: i64,
    task_ids: Vec<i64>,
    priority: Option<TaskPriority>,
) -> Result<(), CommandError> {
    let task_ids = crate::board_mutations::unique_task_ids(task_ids)?;
    write_board(&app, board_id, |data| {
        crate::board_mutations::set_tasks_priority(data, &task_ids, priority)
    })
}
