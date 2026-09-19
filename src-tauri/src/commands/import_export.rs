use crate::{
    models::ExportData,
    state::{load_archives, update_stored_with_pre_import_snapshot, SharedAppData},
};
use tauri::State;

#[tauri::command]
pub fn export_data(state: State<'_, SharedAppData>) -> Result<String, String> {
    load_archives(&state)?;
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let export = ExportData {
        columns: guard.stored.columns.clone(),
        archives: guard.stored.archives.clone(),
        templates: guard.stored.templates.clone(),
        labels: guard.labels.iter().cloned().collect(),
        label_recency: guard.stored.label_recency.clone(),
    };
    serde_json::to_string(&export).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn import_data(state: State<'_, SharedAppData>, json_data: String) -> Result<String, String> {
    let import: ExportData =
        serde_json::from_str(&json_data).map_err(|error| format!("Invalid JSON: {error}"))?;

    let (_, snapshot_path) = update_stored_with_pre_import_snapshot(&state, |data| {
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
