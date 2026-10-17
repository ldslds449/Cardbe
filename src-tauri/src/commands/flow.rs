use crate::{
    errors::CommandError,
    flow::{self, FlowData, FlowLayout, Relationship},
    models::BoardRole,
    state::{lock_app, AppData, SharedAppData},
};
use tauri::State;

fn check_context(data: &AppData, board_id: i64, write: bool) -> Result<(), CommandError> {
    if data.active_board_id != board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    if write
        && data
            .database
            .board_role(board_id)
            .map_err(CommandError::repository)?
            == BoardRole::Viewer
    {
        return Err(CommandError::PermissionDenied);
    }
    Ok(())
}

#[tauri::command]
pub fn get_flow(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<FlowData, CommandError> {
    let guard = lock_app(&state)?;
    check_context(&guard, expected_board_id, false)?;
    guard
        .database
        .read_flow(expected_board_id)
        .map_err(CommandError::repository)
}

#[tauri::command]
pub fn save_flow_layout(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    layout: FlowLayout,
) -> Result<(), CommandError> {
    let mut guard = lock_app(&state)?;
    check_context(&guard, expected_board_id, true)?;
    guard
        .database
        .save_flow_layout(expected_board_id, &layout)
        .map_err(CommandError::repository)
}

#[tauri::command]
pub fn create_task_relationship(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    relationship: Relationship,
) -> Result<Relationship, CommandError> {
    let mut guard = lock_app(&state)?;
    check_context(&guard, expected_board_id, true)?;
    let relationship = flow::normalize(relationship);
    guard
        .database
        .change_relationship(expected_board_id, &relationship, false)
        .map_err(CommandError::repository)?;
    Ok(relationship)
}

#[tauri::command]
pub fn remove_task_relationship(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    relationship: Relationship,
) -> Result<(), CommandError> {
    let mut guard = lock_app(&state)?;
    check_context(&guard, expected_board_id, true)?;
    guard
        .database
        .change_relationship(expected_board_id, &flow::normalize(relationship), true)
        .map_err(CommandError::repository)
}
