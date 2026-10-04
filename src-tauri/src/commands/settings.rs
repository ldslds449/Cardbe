use crate::errors::CommandError;
use crate::{
    commands::iroh_share,
    models::{IrohNetworkSettings, LanguagePreference, Settings},
    state::{update_stored, SharedAppData},
};
use tauri::{AppHandle, State};

#[derive(serde::Serialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryNotice {
    StorageRecovered,
}

#[tauri::command]
pub fn get_settings(state: State<'_, SharedAppData>) -> Result<Settings, CommandError> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    Ok(guard.stored.settings.clone())
}

#[tauri::command]
pub fn set_notify_enabled(
    state: State<'_, SharedAppData>,
    enabled: bool,
) -> Result<(), CommandError> {
    update_stored(&state, |data| {
        data.settings.notify_enabled = enabled;
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn get_iroh_network_settings(
    state: State<'_, SharedAppData>,
) -> Result<IrohNetworkSettings, CommandError> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    Ok(guard.stored.settings.iroh_network.clone())
}

#[tauri::command]
pub async fn set_iroh_network_settings(
    state: State<'_, SharedAppData>,
    network: State<'_, iroh_share::IrohShareState>,
    app_handle: AppHandle,
    settings: IrohNetworkSettings,
) -> Result<Option<CommandError>, CommandError> {
    iroh_share::validate_network_settings(&settings).map_err(CommandError::from)?;
    update_stored(&state, |data| {
        data.settings.iroh_network = settings;
        Ok(())
    })
    .map_err(CommandError::from)?;
    Ok(iroh_share::restart_host_if_running(&network, app_handle)
        .await
        .err()
        .map(CommandError::internal))
}

#[tauri::command]
pub fn set_language(
    state: State<'_, SharedAppData>,
    language: LanguagePreference,
) -> Result<(), CommandError> {
    update_stored(&state, |data| {
        data.settings.language = language;
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_desktop_menu_labels(
    app: tauri::AppHandle,
    labels: [String; 5],
) -> Result<(), CommandError> {
    #[cfg(desktop)]
    {
        crate::desktop::set_menu_labels(&app, labels).map_err(CommandError::from)
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, labels);
        Ok(())
    }
}

#[tauri::command]
pub fn take_recovery_messages(
    state: State<'_, SharedAppData>,
) -> Result<Vec<RecoveryNotice>, CommandError> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let messages = std::mem::take(&mut guard.recovery_messages);
    // Details were logged during startup; notifications never expose paths
    // or backend wording to the UI.
    Ok(if messages.is_empty() {
        Vec::new()
    } else {
        vec![RecoveryNotice::StorageRecovered]
    })
}
