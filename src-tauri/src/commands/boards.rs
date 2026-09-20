use crate::{models::Board, state::SharedAppData};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct BoardsState {
    boards: Vec<Board>,
    active_board_id: i64,
}

fn board_name(value: String) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err("Board name cannot be empty".into())
    } else {
        Ok(value)
    }
}

fn next_board_after_delete(boards: &[Board], removed_id: i64) -> Result<i64, String> {
    if boards.len() <= 1 {
        return Err("At least one board is required".into());
    }
    let index = boards
        .iter()
        .position(|board| board.id == removed_id)
        .ok_or("Board not found")?;
    Ok(boards
        .get(index + 1)
        .or_else(|| boards.get(index.saturating_sub(1)))
        .expect("a board remains after validating length")
        .id)
}

#[tauri::command]
pub fn get_boards(state: State<'_, SharedAppData>) -> Result<BoardsState, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    Ok(BoardsState {
        boards: guard.database.boards().map_err(|e| e.to_string())?,
        active_board_id: guard.active_board_id,
    })
}

#[tauri::command]
pub fn create_board(state: State<'_, SharedAppData>, name: String) -> Result<Board, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let board = guard
        .database
        .create_board(&board_name(name)?)
        .map_err(|e| e.to_string())?;
    guard.boards.push(board.clone());
    Ok(board)
}

#[tauri::command]
pub fn rename_board(
    state: State<'_, SharedAppData>,
    board_id: i64,
    name: String,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let name = board_name(name)?;
    guard
        .database
        .rename_board(board_id, &name)
        .map_err(|e| e.to_string())?;
    let board = guard
        .boards
        .iter_mut()
        .find(|board| board.id == board_id)
        .ok_or("Board not found")?;
    board.name = name;
    Ok(())
}

#[tauri::command]
pub fn switch_board(state: State<'_, SharedAppData>, board_id: i64) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if !guard.boards.iter().any(|board| board.id == board_id) {
        return Err("Board not found".into());
    }
    guard.stored = guard
        .database
        .load_board(board_id)
        .map_err(|e| e.to_string())?;
    guard.active_board_id = board_id;
    guard.undo_history.clear();
    guard.archives_loaded = true;
    guard.refresh_labels();
    Ok(())
}

#[tauri::command]
pub fn delete_board(state: State<'_, SharedAppData>, board_id: i64) -> Result<i64, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let next = next_board_after_delete(&guard.boards, board_id)?;
    let index = guard
        .boards
        .iter()
        .position(|board| board.id == board_id)
        .expect("validated above");
    guard
        .database
        .delete_board(board_id)
        .map_err(|e| e.to_string())?;
    guard.boards.remove(index);
    if board_id == guard.active_board_id {
        guard.stored = guard.database.load_board(next).map_err(|e| e.to_string())?;
        guard.active_board_id = next;
        guard.undo_history.clear();
        guard.refresh_labels();
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(id: i64) -> Board {
        Board {
            id,
            name: format!("Board {id}"),
            task_count: 0,
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
