use crate::errors::CommandError;
use crate::{
    state::{lock_app, SharedAppData},
    task_events::TaskEvent,
};
use tauri::State;

/// The current board's device-local task event log in recording order. With
/// `task_id`, only that task's events and board resets are returned; with
/// `after_id`, only board events recorded after it, so a loaded log can grow.
#[tauri::command]
pub fn get_task_events(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_id: Option<i64>,
    after_id: Option<i64>,
) -> Result<Vec<TaskEvent>, CommandError> {
    let guard = lock_app(&state)?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    let events = match task_id {
        Some(task_id) => guard.database.task_events(expected_board_id, Some(task_id)),
        None => guard
            .database
            .task_events_after(expected_board_id, after_id),
    };
    events.map_err(CommandError::repository)
}
