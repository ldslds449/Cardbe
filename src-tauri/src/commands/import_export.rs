use crate::errors::CommandError;
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
) -> Result<String, CommandError> {
    let export = read_with_archives_for_board(&state, expected_board_id, |guard| ExportData {
        columns: guard.stored.columns.clone(),
        archives: guard.stored.archives.clone(),
        templates: guard.stored.templates.clone(),
        labels: guard.labels.iter().cloned().collect(),
        label_recency: guard.stored.label_recency.clone(),
    })?;
    serde_json::to_string(&export).map_err(CommandError::internal)
}

#[tauri::command]
pub fn export_all_boards(state: State<'_, SharedAppData>) -> Result<String, CommandError> {
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
                .map_err(CommandError::repository)?;
            Ok(AllBoardsExportBoard {
                name: board.name,
                data: ExportData {
                    columns: data.columns,
                    archives: guard
                        .database
                        .load_board_archives_for_export(board.id)
                        .map_err(CommandError::repository)?,
                    templates: data.templates,
                    labels: Vec::new(),
                    label_recency: data.label_recency,
                },
            })
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    let notes = guard
        .database
        .get_notes()
        .map_err(CommandError::repository)?;
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
    .map_err(CommandError::internal)
}

fn parse_all_boards_backup(json_data: &str) -> Result<AllBoardsExport, CommandError> {
    let backup: AllBoardsExport =
        serde_json::from_str(json_data).map_err(|_| CommandError::InvalidImport)?;
    if backup.schema_version != 1 {
        return Err(CommandError::UnsupportedBackupVersion);
    }
    if backup.boards.is_empty() || backup.active_board_index >= backup.boards.len() {
        return Err(CommandError::InvalidImport);
    }
    for board in &backup.boards {
        super::boards::board_name(board.name.clone())
            .map_err(|_| CommandError::BoardNameInvalid)?;
    }
    let mut note_ids = std::collections::HashSet::new();
    if backup.notes.iter().any(|note| !note_ids.insert(note.id)) {
        return Err(CommandError::InvalidImport);
    }
    Ok(backup)
}

#[tauri::command]
pub fn validate_all_boards_backup(json_data: String) -> Result<(), CommandError> {
    parse_all_boards_backup(&json_data).map(|_| ())
}

#[tauri::command]
pub fn import_all_boards(
    state: State<'_, SharedAppData>,
    app_handle: tauri::AppHandle,
    json_data: String,
) -> Result<(), CommandError> {
    let backup = parse_all_boards_backup(&json_data)?;
    let mut prepared = Vec::with_capacity(backup.boards.len());
    for board in backup.boards {
        let name = super::boards::board_name(board.name)?;
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
        .map_err(CommandError::repository)?;
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
) -> Result<String, CommandError> {
    let import: ExportData =
        serde_json::from_str(&json_data).map_err(|_| CommandError::InvalidImport)?;

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
) -> Result<crate::models::Board, CommandError> {
    let import: ExportData =
        serde_json::from_str(&json_data).map_err(|_| CommandError::InvalidImport)?;
    let name = super::boards::board_name(name).map_err(|_| CommandError::BoardNameInvalid)?;
    let mut data = StoredData {
        columns: import.columns,
        archives: import.archives,
        templates: import.templates,
        label_recency: import.label_recency,
        schema_version: CURRENT_SCHEMA_VERSION,
        ..StoredData::default()
    };
    data.repair_column_ids();
    data.repair_task_ids();
    data.repair_template_ids();

    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let board = guard
        .database
        .create_board_with_data(&name, &data)
        .map_err(CommandError::repository)?;
    guard.boards.push(board.clone());
    guard.stored = guard
        .database
        .load_board(board.id)
        .map_err(CommandError::repository)?;
    guard.active_board_id = board.id;
    guard.undo_history.clear();
    guard.archives_loaded = true;
    guard.refresh_labels();
    Ok(board)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backup(name: &str) -> String {
        serde_json::to_string(&AllBoardsExport {
            schema_version: 1,
            boards: vec![AllBoardsExportBoard {
                name: name.to_string(),
                data: ExportData {
                    columns: vec![],
                    archives: vec![],
                    templates: vec![],
                    labels: vec![],
                    label_recency: vec![],
                },
            }],
            active_board_index: 0,
            settings: Default::default(),
            notes: vec![],
        })
        .unwrap()
    }

    #[test]
    fn backup_preserves_existing_unicode_and_long_names() {
        for name in [
            "\u{4e2d}".repeat(67),
            "\u{4e2d}".repeat(200),
            "a".repeat(201),
            "\u{1f600}".repeat(200),
        ] {
            let parsed = parse_all_boards_backup(&backup(&name)).unwrap();
            assert_eq!(parsed.boards[0].name, name);
            assert_eq!(
                super::super::boards::board_name(name.clone()).unwrap(),
                name
            );
        }
    }

    #[test]
    fn preflight_rejects_invalid_nonempty_boards_and_context() {
        assert!(matches!(
            parse_all_boards_backup(r#"{"schema_version":1,"boards":[{}]}"#),
            Err(CommandError::InvalidImport)
        ));
        assert!(parse_all_boards_backup(&backup(" ")).is_err());
        let mut duplicate_notes: serde_json::Value =
            serde_json::from_str(&backup("Board")).unwrap();
        let note = serde_json::json!({"id":1,"title":"","content":"","pinned":false,"created_at":0,"updated_at":0});
        duplicate_notes["notes"] = serde_json::json!([note, note]);
        assert!(matches!(
            parse_all_boards_backup(&duplicate_notes.to_string()),
            Err(CommandError::InvalidImport)
        ));
        let valid = backup("Board");
        let mut value: serde_json::Value = serde_json::from_str(&valid).unwrap();
        value["active_board_index"] = 1.into();
        assert!(matches!(
            parse_all_boards_backup(&value.to_string()),
            Err(CommandError::InvalidImport)
        ));
        value["active_board_index"] = 0.into();
        value["schema_version"] = 2.into();
        assert!(matches!(
            parse_all_boards_backup(&value.to_string()),
            Err(CommandError::UnsupportedBackupVersion)
        ));
    }
}
