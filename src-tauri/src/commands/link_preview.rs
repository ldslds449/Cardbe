use crate::{
    errors::{CommandError, DomainError},
    link_preview::{self, LinkPreview},
    state::SharedAppData,
};
use tauri::State;

#[tauri::command]
pub async fn get_link_preview(
    state: State<'_, SharedAppData>,
    url: String,
) -> Result<LinkPreview, CommandError> {
    let Some(normalized) = link_preview::normalize(&url) else {
        return Ok(LinkPreview::unavailable(&url));
    };
    {
        let guard = state
            .lock()
            .map_err(|_| DomainError::Internal("Application state lock is poisoned".into()))?;
        if let Some(mut preview) = guard
            .database
            .get_link_preview(normalized.as_str(), chrono::Utc::now().timestamp())?
        {
            preview.url = url;
            return Ok(preview);
        }
    }
    let mut preview = link_preview::preview(normalized).await;
    state
        .lock()
        .map_err(|_| DomainError::Internal("Application state lock is poisoned".into()))?
        .database
        .cache_link_preview(&preview)?;
    preview.url = url;
    Ok(preview)
}

#[tauri::command]
pub async fn get_link_preview_image(url: String) -> Result<Option<String>, CommandError> {
    Ok(link_preview::image(&url).await)
}
