use crate::errors::CommandError;
use serde::Serialize;
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const MAX_FRONTEND_MESSAGE_BYTES: usize = 16 * 1024;

pub(crate) fn install_panic_logging() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Panic payloads may contain user data or credentials; record only the call site and stack.
        let diagnostic = redact_diagnostic_logs(format!(
            "Rust panic at {}\n{}",
            info.location()
                .map(|location| location.to_string())
                .unwrap_or_else(|| "unknown location".into()),
            std::backtrace::Backtrace::force_capture(),
        ));
        log::error!(target: "panic", "{diagnostic}");
        log::logger().flush();
        previous_hook(info);
    }));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfo<'a> {
    app_version: String,
    build_commit: Option<String>,
    os: &'a str,
    architecture: &'a str,
    exported_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticInfo {
    app_version: String,
    build_commit: Option<String>,
    os: &'static str,
    architecture: &'static str,
    log_directory: String,
}

fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_log_dir().map_err(|error| error.to_string())
}

fn build_commit() -> Option<String> {
    option_env!("CARDBE_BUILD_COMMIT").and_then(|commit| {
        let commit = commit.trim();
        (!commit.is_empty()).then(|| commit.chars().take(7).collect())
    })
}

#[tauri::command]
pub fn get_diagnostics(app: AppHandle) -> Result<DiagnosticInfo, CommandError> {
    Ok(DiagnosticInfo {
        app_version: app.package_info().version.to_string(),
        build_commit: build_commit(),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        log_directory: log_dir(&app)?.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn log_frontend(level: String, target: String, message: String) {
    let target = if target.starts_with("frontend.") {
        target
    } else {
        "frontend".to_string()
    };
    let mut message = message;
    if message.len() > MAX_FRONTEND_MESSAGE_BYTES {
        let mut end = MAX_FRONTEND_MESSAGE_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str(" [truncated]");
    }

    match level.as_str() {
        "debug" => log::debug!(target: &target, "{message}"),
        "info" => log::info!(target: &target, "{message}"),
        "warn" => log::warn!(target: &target, "{message}"),
        _ => log::error!(target: &target, "{message}"),
    }
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> Result<(), CommandError> {
    let directory = log_dir(&app)?;
    fs::create_dir_all(&directory).map_err(CommandError::internal)?;
    app.opener()
        .open_path(directory.to_string_lossy().into_owned(), None::<&str>)
        .map_err(CommandError::internal)
}

fn log_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut files = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(error) => {
                log::warn!(target: "diagnostics", "Could not inspect a debug log directory entry: {error}");
                None
            }
        })
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|extension| extension == "log")
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

pub(crate) fn redact_diagnostic_logs(mut text: String) -> String {
    for home in [std::env::var("USERPROFILE"), std::env::var("HOME")]
        .into_iter()
        .filter_map(Result::ok)
    {
        let home = home.trim();
        if home.len() >= 4 {
            text = text.replace(home, "[user-path]");
            text = text.replace(&home.replace('\\', "/"), "[user-path]");
        }
    }

    text = redact_user_paths(&text);
    text = redact_urls(&text);
    redact_sensitive_values(&text)
}

fn redact_user_paths(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((start, _)) = ["C:/Users/", "C:\\Users\\", "/Users/", "/home/"]
        .iter()
        .filter_map(|marker| rest.find(marker).map(|start| (start, *marker)))
        .min_by_key(|(start, _)| *start)
    {
        result.push_str(&rest[..start]);
        let end = rest[start..]
            .find(|character: char| character.is_whitespace() || "\"',;)]}".contains(character))
            .map(|offset| start + offset)
            .unwrap_or(rest.len());
        result.push_str("[user-path]");
        rest = &rest[end..];
        if rest.is_empty() {
            break;
        }
    }
    result.push_str(rest);
    result
}

fn redact_urls(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((start, marker)) = ["https://", "http://"]
        .iter()
        .filter_map(|marker| rest.find(marker).map(|start| (start, *marker)))
        .min_by_key(|(start, _)| *start)
    {
        result.push_str(&rest[..start]);
        let end = rest[start + marker.len()..]
            .find(|character: char| character.is_whitespace() || "\"'<>,)]}".contains(character))
            .map(|offset| start + marker.len() + offset)
            .unwrap_or(rest.len());
        result.push_str("[url]");
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

fn redact_sensitive_values(text: &str) -> String {
    const KEYS: [&str; 17] = [
        "token",
        "password",
        "secret",
        "credential",
        "authorization",
        "api_key",
        "apikey",
        "bearer",
        "title",
        "description",
        "content",
        "body",
        "text",
        "document",
        "board",
        "task",
        "note",
    ];

    let lower = text.to_ascii_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let key = KEYS.iter().find(|key| {
            lower[index..].starts_with(**key)
                && (index == 0 || !lower.as_bytes()[index - 1].is_ascii_alphanumeric())
                && (index + key.len() == text.len()
                    || !lower.as_bytes()[index + key.len()].is_ascii_alphanumeric())
        });
        let Some(key) = key else {
            let character = text[index..].chars().next().unwrap();
            result.push(character);
            index += character.len_utf8();
            continue;
        };

        let key_end = index + key.len();
        let mut value_start = key_end;
        if matches!(text.as_bytes().get(value_start), Some(b'"' | b'\'')) {
            value_start += 1;
        }
        while value_start < text.len() && text.as_bytes()[value_start].is_ascii_whitespace() {
            value_start += 1;
        }
        if value_start >= text.len() || !matches!(text.as_bytes()[value_start], b'=' | b':') {
            result.push_str(&text[index..key_end]);
            index = key_end;
            continue;
        }
        value_start += 1;
        while value_start < text.len() && text.as_bytes()[value_start].is_ascii_whitespace() {
            value_start += 1;
        }
        let quoted = text.as_bytes().get(value_start).copied() == Some(b'"')
            || text.as_bytes().get(value_start).copied() == Some(b'\'');
        if quoted {
            value_start += 1;
        }
        let value_end = text[value_start..]
            .find(|character: char| {
                if quoted {
                    character == '"' || character == '\''
                } else {
                    character.is_whitespace() || ",;}]\")".contains(character)
                }
            })
            .map(|offset| value_start + offset)
            .unwrap_or(text.len());
        result.push_str(&text[index..value_start]);
        result.push_str("[redacted]");
        index = value_end;
    }
    result
}

#[tauri::command]
pub fn export_debug_information(app: AppHandle, destination: String) -> Result<(), CommandError> {
    let destination = PathBuf::from(destination);
    if destination.file_name().is_none()
        || !destination
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err(CommandError::InvalidArgument);
    }

    let output = File::create(&destination).map_err(CommandError::internal)?;
    let mut zip = ZipWriter::new(output);
    for source in log_files(&log_dir(&app)?)? {
        if let Some(name) = source.file_name().and_then(|name| name.to_str()) {
            let contents = fs::read_to_string(&source).map_err(CommandError::internal)?;
            zip.start_file(
                format!("logs/{name}"),
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(CommandError::internal)?;
            zip.write_all(redact_diagnostic_logs(contents).as_bytes())
                .map_err(CommandError::internal)?;
        }
    }

    let info = SystemInfo {
        app_version: app.package_info().version.to_string(),
        build_commit: build_commit(),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        exported_at: chrono::Local::now().to_rfc3339(),
    };
    zip.start_file(
        "system-info.json",
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .map_err(CommandError::internal)?;
    zip.write_all(
        serde_json::to_string_pretty(&info)
            .map_err(CommandError::internal)?
            .as_bytes(),
    )
    .map_err(CommandError::internal)?;
    zip.finish().map_err(CommandError::internal)?;
    log::info!(target: "diagnostics", "Exported debug information");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::redact_diagnostic_logs;

    #[test]
    fn panic_logging_probe() {
        let Some(path) = std::env::var_os("CARDBE_PANIC_LOG_TEST_FILE") else {
            return;
        };
        tauri_plugin_log::fern::Dispatch::new()
            .chain(std::fs::File::create(path).unwrap())
            .apply()
            .unwrap();
        super::install_panic_logging();
        panic!("secret-panic-payload");
    }

    #[test]
    fn panic_logging_flushes_location_and_stack_without_payload() {
        let path =
            std::env::temp_dir().join(format!("cardbe-panic-log-{}.txt", std::process::id()));
        assert!(!path.exists());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "commands::diagnostics::tests::panic_logging_probe",
            ])
            .env("CARDBE_PANIC_LOG_TEST_FILE", &path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let logged = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(logged.contains("Rust panic at"));
        assert!(logged.contains("diagnostics.rs:"));
        assert!(logged.contains("panic_logging_probe"));
        assert!(!logged.contains("secret-panic-payload"));
    }

    #[test]
    fn redacts_paths_urls_and_secret_values() {
        let redacted = redact_diagnostic_logs(
            "failed C:/Users/Alice/private-board.ts token=secret-value https://private.example/invite {\"title\":\"Private board\"}"
                .into(),
        );
        assert!(!redacted.contains("Alice"));
        assert!(!redacted.contains("secret-value"));
        assert!(!redacted.contains("private.example"));
        assert!(!redacted.contains("Private board"));
        assert!(redacted.contains("[user-path]"));
        assert!(redacted.contains("[redacted]"));
        assert!(redacted.contains("[url]"));
    }
}
