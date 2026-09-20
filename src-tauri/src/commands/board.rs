use crate::{
    models::{Column, ColumnSort, StoredData, Task},
    state::{undo_last_change_for_board, update_stored_for_board, SharedAppData},
};
use tauri::State;

fn column_index(data: &StoredData, column_id: i64) -> Result<usize, String> {
    data.columns
        .iter()
        .position(|column| column.id == column_id)
        .ok_or_else(|| format!("Column not found: {column_id}"))
}

fn task_position(data: &StoredData, task_id: i64) -> Result<(usize, usize), String> {
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
        .ok_or_else(|| format!("Task not found: {task_id}"))
}

#[tauri::command]
pub fn get_columns(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<Column>, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    Ok(guard.stored.columns.clone())
}

#[tauri::command]
pub fn get_board_columns(
    state: State<'_, SharedAppData>,
    board_id: i64,
) -> Result<Vec<Column>, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if !guard
        .database
        .board_exists(board_id)
        .map_err(|e| e.to_string())?
    {
        return Err("Board not found".into());
    }
    Ok(guard
        .database
        .read_board(board_id)
        .map_err(|e| e.to_string())?
        .columns)
}

#[tauri::command]
pub fn get_labels(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<String>, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    Ok(guard.ordered_labels())
}

#[tauri::command]
pub fn undo(state: State<'_, SharedAppData>, expected_board_id: i64) -> Result<bool, String> {
    undo_last_change_for_board(&state, expected_board_id)
}

#[tauri::command]
pub fn add_column(
    state: State<'_, SharedAppData>,
    name: String,
    color: String,
    expected_board_id: i64,
) -> Result<i64, String> {
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
}

#[tauri::command]
pub fn update_column(
    state: State<'_, SharedAppData>,
    column_id: i64,
    name: String,
    color: String,
    sort_order: ColumnSort,
    expected_board_id: i64,
) -> Result<(), String> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let index = column_index(data, column_id)?;
        data.columns[index].name = name;
        data.columns[index].color = color;
        data.columns[index].sort_order = sort_order;
        Ok(())
    })
}

#[tauri::command]
pub fn delete_column(
    state: State<'_, SharedAppData>,
    column_id: i64,
    expected_board_id: i64,
) -> Result<(), String> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let index = column_index(data, column_id)?;
        data.columns.remove(index);
        Ok(())
    })
}

#[tauri::command]
pub async fn move_column(
    state: State<'_, SharedAppData>,
    column_id: i64,
    before_column_id: Option<i64>,
    expected_board_id: i64,
) -> Result<(), String> {
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
}

#[tauri::command]
pub async fn move_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    to_column_id: i64,
    before_task_id: Option<i64>,
    expected_board_id: i64,
) -> Result<(), String> {
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
                .ok_or_else(|| format!("Destination task not found in column: {id}"))?,
            None => data.columns[to_column].tasks.len(),
        };
        data.columns[to_column].tasks.insert(destination, task);
        Ok(())
    })
}

#[tauri::command]
pub fn add_task(
    state: State<'_, SharedAppData>,
    column_id: i64,
    mut task: Task,
    after_task_id: Option<i64>,
    expected_board_id: i64,
) -> Result<i64, String> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let column = column_index(data, column_id)?;
        let destination = match after_task_id {
            Some(id) => {
                data.columns[column]
                    .tasks
                    .iter()
                    .position(|candidate| candidate.id == id)
                    .ok_or_else(|| format!("Original task not found in column: {id}"))?
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
}

/// Quick Add deliberately targets an explicit board and never changes global
/// active-board state as a side effect.
#[tauri::command]
pub fn add_task_to_board(
    state: State<'_, SharedAppData>,
    board_id: i64,
    column_id: i64,
    mut task: Task,
) -> Result<i64, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if !guard
        .database
        .board_exists(board_id)
        .map_err(|e| e.to_string())?
    {
        return Err("Target board no longer exists".into());
    }
    let mut data = guard
        .database
        .read_board_complete(board_id)
        .map_err(|e| e.to_string())?;
    let column = column_index(&data, column_id)?;
    let id = data.allocate_task_id()?;
    task.id = id;
    for label in &task.labels {
        data.touch_label(label.clone());
    }
    data.columns[column].tasks.push(task);
    guard
        .database
        .replace_board(board_id, &data)
        .map_err(|e| e.to_string())?;
    if board_id == guard.active_board_id {
        // Preserve the lazily-loaded archive contract for the active in-memory
        // view while the complete database write keeps archives intact.
        if !guard.archives_loaded {
            data.archives.clear();
        }
        guard.stored = data;
        guard.refresh_labels();
    }
    if let Some(board) = guard.boards.iter_mut().find(|board| board.id == board_id) {
        board.task_count += 1;
    }
    Ok(id)
}

#[tauri::command]
pub fn delete_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    expected_board_id: i64,
) -> Result<(), String> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let (column, task) = task_position(data, task_id)?;
        data.columns[column].tasks.remove(task);
        Ok(())
    })
}

#[tauri::command]
pub fn update_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    mut task: Task,
    expected_board_id: i64,
) -> Result<(), String> {
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
}
