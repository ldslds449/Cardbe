use serde::Serialize;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const MAX_FRONTEND_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfo<'a> {
    app_version: String,
    os: &'a str,
    architecture: &'a str,
    exported_at: String,
}

fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_log_dir().map_err(|error| error.to_string())
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
pub fn open_log_folder(app: AppHandle) -> Result<(), String> {
    let directory = log_dir(&app)?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    app.opener()
        .open_path(directory.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| error.to_string())
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

fn add_file(zip: &mut ZipWriter<File>, source: &Path, name: &str) -> Result<(), String> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(name, options)
        .map_err(|error| error.to_string())?;
    let mut input = File::open(source).map_err(|error| error.to_string())?;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        zip.write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn export_debug_logs(app: AppHandle, destination: String) -> Result<(), String> {
    let destination = PathBuf::from(destination);
    if destination.file_name().is_none()
        || !destination
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err("Invalid debug log export destination".to_string());
    }

    let output = File::create(&destination).map_err(|error| error.to_string())?;
    let mut zip = ZipWriter::new(output);
    for source in log_files(&log_dir(&app)?)? {
        if let Some(name) = source.file_name().and_then(|name| name.to_str()) {
            add_file(&mut zip, &source, &format!("logs/{name}"))?;
        }
    }

    let info = SystemInfo {
        app_version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        exported_at: chrono::Local::now().to_rfc3339(),
    };
    zip.start_file(
        "system-info.json",
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .map_err(|error| error.to_string())?;
    zip.write_all(
        serde_json::to_string_pretty(&info)
            .map_err(|error| error.to_string())?
            .as_bytes(),
    )
    .map_err(|error| error.to_string())?;
    zip.finish().map_err(|error| error.to_string())?;
    log::info!(target: "diagnostics", "Exported debug logs");
    Ok(())
}
