use crate::errors::CommandError;
use crate::{
    commands::iroh_share,
    models::{IrohNetworkSettings, LanguagePreference, Settings},
    state::{lock_app, update_stored, SharedAppData},
};
use tauri::{AppHandle, Manager, State};

#[derive(serde::Serialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryNotice {
    StorageRecovered,
}

#[tauri::command]
pub async fn get_settings(
    app: AppHandle,
    startup: State<'_, crate::StartupError>,
) -> Result<Settings, CommandError> {
    // Webviews load before setup manages application data; wait instead of failing.
    if let Some(error) = startup.wait().await? {
        return Err(error);
    }
    let state = app
        .try_state::<SharedAppData>()
        .ok_or_else(|| CommandError::internal("Application data unavailable"))?;
    let guard = lock_app(&state)?;
    Ok(guard.stored.settings.clone())
}

#[tauri::command]
pub fn set_notify_enabled(
    state: State<'_, SharedAppData>,
    enabled: bool,
) -> Result<(), CommandError> {
    {
        let mut guard = lock_app(&state)?;
        if enabled && !guard.stored.settings.notify_enabled {
            // Tasks that became overdue while notifications were off stay silent.
            let now = crate::commands::notifications::current_time_millis()?;
            guard
                .database
                .set_notifications_checked_until(now)
                .map_err(CommandError::repository)?;
        }
    }
    update_stored(&state, |data| {
        data.settings.notify_enabled = enabled;
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_column_notify_muted(
    state: State<'_, SharedAppData>,
    board_id: i64,
    column_id: i64,
    muted: bool,
) -> Result<(), CommandError> {
    // Device-local like other notification settings, so viewers may change it.
    if !lock_app(&state)?
        .boards
        .iter()
        .any(|board| board.id == board_id)
    {
        return Err(crate::errors::DomainError::BoardNotFound.into());
    }
    update_stored(&state, |data| {
        let muted_columns = &mut data.settings.notify_muted_columns;
        if muted {
            muted_columns.entry(board_id).or_default().insert(column_id);
        } else if let Some(columns) = muted_columns.get_mut(&board_id) {
            columns.remove(&column_id);
            if columns.is_empty() {
                muted_columns.remove(&board_id);
            }
        }
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_task_expand_mode(
    state: State<'_, SharedAppData>,
    board_id: i64,
    expanded: bool,
) -> Result<(), CommandError> {
    // A view preference, so viewers of shared boards may change it too.
    if !lock_app(&state)?
        .boards
        .iter()
        .any(|board| board.id == board_id)
    {
        return Err(crate::errors::DomainError::BoardNotFound.into());
    }
    update_stored(&state, |data| {
        data.settings
            .board_task_expand_modes
            .insert(board_id, expanded);
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn get_iroh_network_settings(
    state: State<'_, SharedAppData>,
) -> Result<IrohNetworkSettings, CommandError> {
    let guard = lock_app(&state)?;
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

fn valid_theme(theme: &str) -> bool {
    matches!(
        theme,
        "system"
            | "light"
            | "dark"
            | "morandi-sage"
            | "morandi-rose"
            | "morandi-blue"
            | "morandi-sand"
            | "morandi-dark-sage"
            | "morandi-dark-blue"
            | "morandi-dark-mauve"
            | "morandi-dark-cocoa"
    )
}

#[tauri::command]
pub fn set_theme(state: State<'_, SharedAppData>, theme: String) -> Result<(), CommandError> {
    if !valid_theme(&theme) {
        return Err(crate::errors::DomainError::InvalidArgument.into());
    }
    update_stored(&state, |data| {
        data.settings.theme = Some(theme);
        Ok(())
    })
    .map_err(CommandError::from)
}

/// Tints the native title bar with the active theme; unsupported systems keep the
/// light/dark caption chosen from the window theme.
#[tauri::command]
pub fn set_window_caption_color(
    window: tauri::WebviewWindow,
    caption: [u8; 3],
    text: [u8; 3],
) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Dwm::{DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR};
        let hwnd = window.hwnd().map_err(CommandError::internal)?.0;
        for (attribute, color) in [(DWMWA_CAPTION_COLOR, caption), (DWMWA_TEXT_COLOR, text)] {
            if let Err(code) = set_dwm_color(hwnd, attribute as u32, color) {
                // Caption colors require Windows 11; older systems reject the attribute.
                log::debug!(target: "theme", "caption color unavailable: {code:#x}");
                break;
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (window, caption, text);
        Ok(())
    }
}

#[cfg(windows)]
fn set_dwm_color(
    hwnd: *mut std::ffi::c_void,
    attribute: u32,
    [red, green, blue]: [u8; 3],
) -> Result<(), i32> {
    let color = colorref([red, green, blue]);
    // SAFETY: `hwnd` belongs to the live window that invoked this command, and the
    // pointer and size describe a COLORREF that outlives the call.
    let result = unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            attribute,
            std::ptr::from_ref(&color).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    if result < 0 {
        Err(result)
    } else {
        Ok(())
    }
}

#[cfg(any(windows, test))]
fn colorref([red, green, blue]: [u8; 3]) -> u32 {
    u32::from(red) | u32::from(green) << 8 | u32::from(blue) << 16
}

#[derive(serde::Serialize)]
pub struct GlobalShortcutStatus {
    pub action: crate::models::ShortcutAction,
    /// `None` when the user turned this shortcut off.
    pub shortcut: Option<String>,
    pub default_shortcut: &'static str,
    /// False when global shortcuts are off or another app owns the keys.
    pub active: bool,
}

#[derive(serde::Serialize)]
pub struct GlobalShortcutOverview {
    /// The group switch shared with the tray menu.
    pub enabled: bool,
    pub shortcuts: Vec<GlobalShortcutStatus>,
}

#[tauri::command]
pub fn get_global_shortcuts(app: AppHandle) -> Result<GlobalShortcutOverview, CommandError> {
    #[cfg(desktop)]
    {
        crate::global_shortcuts::overview(&app)
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(GlobalShortcutOverview {
            enabled: false,
            shortcuts: Vec::new(),
        })
    }
}

#[tauri::command]
pub fn set_global_shortcuts_enabled(
    app: AppHandle,
    enabled: bool,
) -> Result<GlobalShortcutOverview, CommandError> {
    #[cfg(desktop)]
    {
        crate::global_shortcuts::set_enabled(&app, enabled)
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
        Err(CommandError::InvalidArgument)
    }
}

/// Saves one device-local global shortcut; an empty value turns it off.
/// `take_over` moves a combination from the action that currently uses it.
#[tauri::command]
pub fn set_global_shortcut(
    app: AppHandle,
    action: crate::models::ShortcutAction,
    shortcut: String,
    take_over: Option<bool>,
) -> Result<GlobalShortcutOverview, CommandError> {
    #[cfg(desktop)]
    {
        crate::global_shortcuts::set(&app, action, &shortcut, take_over.unwrap_or(false))
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, action, shortcut, take_over);
        Err(CommandError::InvalidArgument)
    }
}

#[cfg(test)]
mod theme_tests {
    #[test]
    fn caption_colors_use_colorref_byte_order() {
        assert_eq!(super::colorref([0x12, 0x34, 0x56]), 0x0056_3412);
    }

    #[test]
    fn theme_allowlist_and_legacy_settings() {
        for id in [
            "system",
            "light",
            "dark",
            "morandi-sage",
            "morandi-rose",
            "morandi-blue",
            "morandi-sand",
            "morandi-dark-sage",
            "morandi-dark-blue",
            "morandi-dark-mauve",
            "morandi-dark-cocoa",
        ] {
            assert!(super::valid_theme(id));
        }
        assert!(!super::valid_theme("custom"));
        let legacy: crate::models::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.theme, None);
        let mut settings = legacy;
        settings.theme = Some("morandi-dark-sage".into());
        let restored: crate::models::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored, settings);
    }
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
pub async fn set_desktop_menu_labels(
    app: tauri::AppHandle,
    labels: [String; 6],
    startup: State<'_, crate::StartupError>,
) -> Result<(), CommandError> {
    #[cfg(desktop)]
    {
        // The menu also exists when storage failed, so still localize its labels.
        let _ = startup.wait().await?;
        crate::desktop::set_menu_labels(&app, labels).map_err(CommandError::from)
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, labels, startup);
        Ok(())
    }
}

#[tauri::command]
pub fn take_recovery_messages(
    state: State<'_, SharedAppData>,
) -> Result<Vec<RecoveryNotice>, CommandError> {
    let mut guard = lock_app(&state)?;
    let messages = std::mem::take(&mut guard.recovery_messages);
    // Details were logged during startup; notifications never expose paths
    // or backend wording to the UI.
    Ok(if messages.is_empty() {
        Vec::new()
    } else {
        vec![RecoveryNotice::StorageRecovered]
    })
}
