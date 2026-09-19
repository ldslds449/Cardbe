use crate::{
    models::Settings,
    state::{update_stored, SharedAppData},
};
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, SharedAppData>) -> Result<Settings, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    Ok(guard.stored.settings.clone())
}

#[tauri::command]
pub fn set_notify_enabled(state: State<'_, SharedAppData>, enabled: bool) -> Result<(), String> {
    update_stored(&state, |data| {
        data.settings.notify_enabled = enabled;
        Ok(())
    })
}

#[tauri::command]
pub fn take_recovery_messages(state: State<'_, SharedAppData>) -> Result<Vec<String>, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    Ok(std::mem::take(&mut guard.recovery_messages))
}
