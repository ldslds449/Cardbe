use crate::{models::Note, state::SharedAppData};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

fn now_millis() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    i64::try_from(millis).map_err(|_| "System time is out of range".to_string())
}

#[tauri::command]
pub fn get_notes(state: State<'_, SharedAppData>) -> Result<Vec<Note>, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    guard
        .database
        .get_notes()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_note(state: State<'_, SharedAppData>) -> Result<Note, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    guard
        .database
        .create_note(now_millis()?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_quick_note(
    state: State<'_, SharedAppData>,
    title: String,
    content: String,
) -> Result<Note, String> {
    let title = title.trim().to_string();
    let content = content.trim().to_string();
    if title.is_empty() && content.is_empty() {
        return Err("A note needs a title or some content".to_string());
    }

    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    guard
        .database
        .create_note_with_content(now_millis()?, title, content)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_note(
    state: State<'_, SharedAppData>,
    id: i64,
    title: String,
    content: String,
    pinned: bool,
) -> Result<Note, String> {
    let mut note = Note {
        id,
        title,
        content,
        pinned,
        created_at: 0,
        updated_at: now_millis()?,
    };
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let existing = guard
        .database
        .get_notes()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| format!("Note not found: {id}"))?;
    note.created_at = existing.created_at;
    guard
        .database
        .update_note(&note)
        .map_err(|error| error.to_string())?;
    Ok(note)
}

#[tauri::command]
pub fn delete_note(state: State<'_, SharedAppData>, id: i64) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    guard
        .database
        .delete_note(id)
        .map_err(|error| error.to_string())
}
