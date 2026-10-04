use crate::errors::{CommandError, DomainError};
use crate::{
    models::{Board, BoardRole},
    state::SharedAppData,
};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct BoardsState {
    boards: Vec<Board>,
    active_board_id: i64,
}

pub(super) fn board_name(value: String) -> Result<String, DomainError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(DomainError::BoardNameRequired)
    } else {
        Ok(value)
    }
}

fn next_board_after_delete(boards: &[Board], removed_id: i64) -> Result<i64, DomainError> {
    if boards.len() <= 1 {
        return Err(DomainError::LastBoardRequired);
    }
    let index = boards
        .iter()
        .position(|board| board.id == removed_id)
        .ok_or(DomainError::BoardNotFound)?;
    Ok(boards
        .get(index + 1)
        .or_else(|| boards.get(index.saturating_sub(1)))
        .expect("a board remains after validating length")
        .id)
}

#[tauri::command]
pub fn get_boards(state: State<'_, SharedAppData>) -> Result<BoardsState, CommandError> {
    let guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    Ok(BoardsState {
        boards: guard.database.boards().map_err(CommandError::repository)?,
        active_board_id: guard.active_board_id,
    })
}

#[tauri::command]
pub fn create_board(state: State<'_, SharedAppData>, name: String) -> Result<Board, CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    let board = guard
        .database
        .create_board(&board_name(name)?)
        .map_err(CommandError::repository)?;
    guard.boards.push(board.clone());
    Ok(board)
}

#[tauri::command]
pub fn rename_board(
    state: State<'_, SharedAppData>,
    board_id: i64,
    name: String,
) -> Result<(), CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    let name = board_name(name)?;
    if guard
        .database
        .board_role(board_id)
        .map_err(CommandError::repository)?
        != BoardRole::Owner
    {
        return Err(CommandError::PermissionDenied);
    }
    guard
        .database
        .rename_board(board_id, &name)
        .map_err(CommandError::repository)?;
    let (revision, status) = guard
        .database
        .board_sync_state(board_id)
        .map_err(CommandError::repository)?;
    let board = guard
        .boards
        .iter_mut()
        .find(|board| board.id == board_id)
        .ok_or(DomainError::BoardNotFound)?;
    board.name = name;
    board.sync_revision = revision;
    board.sync_status = status;
    Ok(())
}

#[tauri::command]
pub fn switch_board(state: State<'_, SharedAppData>, board_id: i64) -> Result<(), CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    if !guard.boards.iter().any(|board| board.id == board_id) {
        return Err(CommandError::BoardNotFound);
    }
    guard.stored = guard
        .database
        .load_board(board_id)
        .map_err(CommandError::repository)?;
    guard.active_board_id = board_id;
    guard.undo_history.clear();
    guard.archives_loaded = true;
    guard.refresh_labels();
    Ok(())
}

#[tauri::command]
pub fn delete_board(state: State<'_, SharedAppData>, board_id: i64) -> Result<i64, CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    let next = next_board_after_delete(&guard.boards, board_id)?;
    let index = guard
        .boards
        .iter()
        .position(|board| board.id == board_id)
        .expect("validated above");
    guard
        .database
        .delete_board(board_id)
        .map_err(CommandError::repository)?;
    guard.boards.remove(index);
    if board_id == guard.active_board_id {
        guard.stored = guard
            .database
            .load_board(next)
            .map_err(CommandError::repository)?;
        guard.active_board_id = next;
        guard.undo_history.clear();
        guard.refresh_labels();
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SyncStatus;

    fn board(id: i64) -> Board {
        Board {
            id,
            name: format!("Board {id}"),
            task_count: 0,
            shared_role: BoardRole::Owner,
            is_shared: false,
            sync_status: SyncStatus::Local,
            sync_revision: 0,
        }
    }

    #[test]
    fn deleting_active_board_selects_a_neighbor_and_rejects_last_board() {
        let boards = vec![board(1), board(2), board(3)];
        assert_eq!(next_board_after_delete(&boards, 2).unwrap(), 3);
        assert_eq!(next_board_after_delete(&boards, 3).unwrap(), 2);
        assert!(next_board_after_delete(&[board(1)], 1).is_err());
    }

    #[test]
    fn boards_state_serializes_the_active_board() {
        let value = serde_json::to_value(BoardsState {
            boards: vec![board(7)],
            active_board_id: 7,
        })
        .unwrap();
        assert_eq!(value["active_board_id"], 7);
        assert_eq!(value["boards"][0]["id"], 7);
        assert_eq!(value["boards"][0]["task_count"], 0);
    }
}
