use crate::{
    errors::CommandError,
    flow::{self, FlowData, FlowLayout, FlowTab, Relationship},
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
    tab_id: Option<String>,
) -> Result<FlowData, CommandError> {
    let guard = lock_app(&state)?;
    check_context(&guard, expected_board_id, false)?;
    guard
        .database
        .read_flow(expected_board_id, tab_id.as_deref())
        .map_err(CommandError::repository)
}

#[tauri::command]
pub fn save_flow_layout(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    tab_id: String,
    layout: FlowLayout,
) -> Result<(), CommandError> {
    let mut guard = lock_app(&state)?;
    check_context(&guard, expected_board_id, true)?;
    guard
        .database
        .save_flow_layout(expected_board_id, &tab_id, &layout)
        .map_err(CommandError::repository)
}

fn change_tabs(
    state: State<'_, SharedAppData>,
    board_id: i64,
    change: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<(), Box<dyn std::error::Error>>,
) -> Result<Vec<FlowTab>, CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    check_context(&guard, board_id, true)?;
    guard
        .database
        .change_flow_tabs(board_id, change)
        .map_err(CommandError::repository)
}

#[tauri::command]
pub fn create_flow_tab(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    tab_id: String,
    name: String,
) -> Result<Vec<FlowTab>, CommandError> {
    change_tabs(state, expected_board_id, |tx| {
        flow::create_tab(tx, expected_board_id, &tab_id, &name)
    })
}

#[tauri::command]
pub fn rename_flow_tab(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    tab_id: String,
    name: String,
) -> Result<Vec<FlowTab>, CommandError> {
    change_tabs(state, expected_board_id, |tx| {
        flow::rename_tab(tx, expected_board_id, &tab_id, &name)
    })
}

#[tauri::command]
pub fn delete_flow_tab(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    tab_id: String,
) -> Result<Vec<FlowTab>, CommandError> {
    change_tabs(state, expected_board_id, |tx| {
        flow::delete_tab(tx, expected_board_id, &tab_id)
    })
}

#[tauri::command]
pub fn reorder_flow_tabs(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    tab_ids: Vec<String>,
) -> Result<Vec<FlowTab>, CommandError> {
    change_tabs(state, expected_board_id, |tx| {
        flow::reorder_tabs(tx, expected_board_id, &tab_ids)
    })
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
pub fn change_task_relationship(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    relationship: Relationship,
) -> Result<Relationship, CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    check_context(&guard, expected_board_id, true)?;
    let relationship = flow::normalize(relationship);
    guard
        .database
        .update_relationship(expected_board_id, &relationship)
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
