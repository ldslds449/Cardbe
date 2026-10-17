use crate::errors::CommandError;
use crate::state::SharedAppData;
use tauri::{AppHandle, State};

fn ensure_task_exists(
    state: &State<'_, SharedAppData>,
    board_id: i64,
    task_id: i64,
) -> Result<(), CommandError> {
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
    guard
        .database
        .load_task(board_id, task_id)
        .map_err(CommandError::repository)?
        .map(|_| ())
        .ok_or(CommandError::TaskNotFound)
}

// Async so that window creation does not deadlock the Windows event loop.
#[tauri::command]
pub async fn open_floating_task(
    app: AppHandle,
    state: State<'_, SharedAppData>,
    board_id: i64,
    task_id: i64,
) -> Result<(), CommandError> {
    ensure_task_exists(&state, board_id, task_id)?;
    #[cfg(desktop)]
    crate::desktop::open_floating_task_window(&app, board_id, task_id)
        .map_err(CommandError::internal)?;
    #[cfg(not(desktop))]
    let _ = app;
    Ok(())
}

#[tauri::command]
pub async fn open_task_in_main_window(
    app: AppHandle,
    state: State<'_, SharedAppData>,
    board_id: i64,
    task_id: i64,
) -> Result<(), CommandError> {
    ensure_task_exists(&state, board_id, task_id)?;
    #[cfg(desktop)]
    crate::desktop::open_task_in_main_window(&app, board_id, task_id)
        .map_err(CommandError::internal)?;
    #[cfg(not(desktop))]
    let _ = app;
    Ok(())
}
