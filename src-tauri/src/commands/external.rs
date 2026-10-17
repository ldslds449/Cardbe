use crate::errors::CommandError;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

fn is_allowed_external_url(value: &str) -> bool {
    tauri::Url::parse(value)
        .map(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or(false)
}

#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> Result<(), CommandError> {
    if !is_allowed_external_url(&url) {
        return Err(CommandError::ExternalUrlInvalid);
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(CommandError::internal)
}

#[cfg(test)]
mod tests {
    use super::is_allowed_external_url;

    #[test]
    fn allows_only_http_and_https_external_urls() {
        assert!(is_allowed_external_url("https://example.com/docs"));
        assert!(is_allowed_external_url("http://localhost:3000/path"));
        assert!(!is_allowed_external_url("javascript:alert(1)"));
        assert!(!is_allowed_external_url("file:///etc/passwd"));
        assert!(!is_allowed_external_url("not a url"));
    }
}
