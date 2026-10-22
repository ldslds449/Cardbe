use crate::errors::CommandError;
use crate::{
    state::{lock_app, SharedAppData},
    task_events::TaskEvent,
};
use tauri::State;

/// The current board's device-local task event log in recording order. With
/// `task_id`, only that task's events and board resets are returned.
#[tauri::command]
pub fn get_task_events(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_id: Option<i64>,
) -> Result<Vec<TaskEvent>, CommandError> {
    let guard = lock_app(&state)?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    guard
        .database
        .task_events(expected_board_id, task_id)
        .map_err(CommandError::repository)
}
