use crate::{
    models::{
        AllBoardsExport, AllBoardsExportBoard, ExportData, StoredData, CURRENT_SCHEMA_VERSION,
    },
    state::{
        read_with_archives_for_board, update_stored_with_pre_import_snapshot_for_board,
        SharedAppData,
    },
};
use tauri::State;

#[tauri::command]
pub fn export_data(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<String, String> {
    let export = read_with_archives_for_board(&state, expected_board_id, |guard| ExportData {
        columns: guard.stored.columns.clone(),
        archives: guard.stored.archives.clone(),
        templates: guard.stored.templates.clone(),
        labels: guard.labels.iter().cloned().collect(),
        label_recency: guard.stored.label_recency.clone(),
    })?;
    serde_json::to_string(&export).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn export_all_boards(state: State<'_, SharedAppData>) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let active_board_id = guard.active_board_id;
    let boards = guard
        .boards
        .clone()
        .into_iter()
        .map(|board| {
            let data = guard
                .database
                .read_board(board.id)
                .map_err(|error| error.to_string())?;
            Ok(AllBoardsExportBoard {
                name: board.name,
                data: ExportData {
                    columns: data.columns,
                    archives: guard
                        .database
                        .load_board_archives_for_export(board.id)
                        .map_err(|error| error.to_string())?,
                    templates: data.templates,
                    labels: Vec::new(),
                    label_recency: data.label_recency,
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let notes = guard
        .database
        .get_notes()
        .map_err(|error| error.to_string())?;
    let active_board_index = guard
        .boards
        .iter()
        .position(|board| board.id == active_board_id)
        .unwrap_or(0);
    serde_json::to_string(&AllBoardsExport {
        schema_version: 1,
        boards,
        active_board_index,
        settings: guard.stored.settings.clone(),
        notes,
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn import_all_boards(
    state: State<'_, SharedAppData>,
    app_handle: tauri::AppHandle,
    json_data: String,
) -> Result<(), String> {
    let backup: AllBoardsExport = serde_json::from_str(&json_data)
        .map_err(|error| format!("Invalid all-board backup: {error}"))?;
    if backup.schema_version != 1 {
        return Err("Unsupported all-board backup version".into());
    }
    if backup.boards.is_empty() || backup.active_board_index >= backup.boards.len() {
        return Err("Backup has no valid active board".into());
    }
    let mut prepared = Vec::with_capacity(backup.boards.len());
    for board in backup.boards {
        let name = board.name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err("Backup contains an invalid board name".into());
        }
        let mut data = StoredData::default();
        data.columns = board.data.columns;
        data.archives = board.data.archives;
        data.templates = board.data.templates;
        data.label_recency = board.data.label_recency;
        data.schema_version = CURRENT_SCHEMA_VERSION;
        data.repair_column_ids();
        data.repair_task_ids();
        data.repair_template_ids();
        prepared.push((name.to_string(), data));
    }
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let (boards, stored) = guard
        .database
        .replace_all_boards(
            &prepared,
            backup.active_board_index,
            &backup.notes,
            &backup.settings,
        )
        .map_err(|error| error.to_string())?;
    guard.boards = boards;
    guard.active_board_id = guard.boards[backup.active_board_index].id;
    guard.stored = stored;
    guard.undo_history.clear();
    guard.refresh_labels();
    guard.archives_loaded = true;
    drop(guard);
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        let network = app_handle.state::<crate::commands::iroh_share::IrohShareState>();
        if let Err(error) = crate::commands::iroh_share::stop_host_if_idle(&network).await {
            log::warn!(target: "iroh", "Could not stop board sharing after importing data: {error}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn import_data(
    state: State<'_, SharedAppData>,
    json_data: String,
    expected_board_id: i64,
) -> Result<String, String> {
    let import: ExportData =
        serde_json::from_str(&json_data).map_err(|error| format!("Invalid JSON: {error}"))?;

    let (_, snapshot_path) =
        update_stored_with_pre_import_snapshot_for_board(&state, expected_board_id, |data| {
            data.columns = import.columns;
            data.archives = import.archives;
            data.templates = import.templates;
            data.label_recency = import.label_recency;
            data.next_column_id = 0;
            data.next_task_id = 0;
            data.next_template_id = 0;
            data.repair_column_ids();
            data.repair_task_ids();
            data.repair_template_ids();
            Ok(())
        })?;

    Ok(snapshot_path.display().to_string())
}

#[tauri::command]
pub fn import_board_as_new(
    state: State<'_, SharedAppData>,
    json_data: String,
    name: String,
) -> Result<crate::models::Board, String> {
    let import: ExportData = serde_json::from_str(&json_data)
        .map_err(|error| format!("Invalid board export: {error}"))?;
    let name = name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err("Invalid board name".into());
    }
    let mut data = StoredData::default();
    data.columns = import.columns;
    data.archives = import.archives;
    data.templates = import.templates;
    data.label_recency = import.label_recency;
    data.schema_version = CURRENT_SCHEMA_VERSION;
    data.repair_column_ids();
    data.repair_task_ids();
    data.repair_template_ids();

    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let board = guard
        .database
        .create_board_with_data(name, &data)
        .map_err(|e| e.to_string())?;
    guard.boards.push(board.clone());
    guard.stored = guard
        .database
        .load_board(board.id)
        .map_err(|e| e.to_string())?;
    guard.active_board_id = board.id;
    guard.undo_history.clear();
    guard.archives_loaded = true;
    guard.refresh_labels();
    Ok(board)
}
