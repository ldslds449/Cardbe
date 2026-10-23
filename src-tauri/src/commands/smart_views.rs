use crate::errors::CommandError;
use crate::{
    smart_views::{self, SmartView},
    state::{lock_app, SharedAppData},
};
use tauri::State;

/// `None` until views were first saved.
#[tauri::command]
pub fn get_smart_views(
    state: State<'_, SharedAppData>,
) -> Result<Option<Vec<SmartView>>, CommandError> {
    let guard = lock_app(&state)?;
    smart_views::load(&guard.database.connection).map_err(CommandError::from)
}

/// Replaces the saved views, in display order.
#[tauri::command]
pub fn set_smart_views(
    state: State<'_, SharedAppData>,
    views: Vec<SmartView>,
) -> Result<(), CommandError> {
    let guard = lock_app(&state)?;
    smart_views::save(&guard.database.connection, &views).map_err(CommandError::from)
}
