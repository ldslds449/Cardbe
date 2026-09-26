mod commands;
#[cfg(desktop)]
mod desktop;
mod loro_board;
mod models;
mod state;
mod storage;

use commands::{
    archive, board, boards, calendar_export, import_export, iroh_share, notes, notifications,
    settings, share, templates, update,
};
use state::AppData;
#[cfg(all(debug_assertions, desktop))]
use std::fs::{self, File, OpenOptions, TryLockError};
#[cfg(all(debug_assertions, desktop))]
use std::io;
#[cfg(all(debug_assertions, desktop))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

#[cfg(all(debug_assertions, desktop))]
fn debug_port() -> io::Result<u16> {
    let port = match std::env::var("CARDBE_DEV_PORT") {
        Ok(port) => port,
        Err(std::env::VarError::NotPresent) => return Ok(1420),
        Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidInput, error)),
    };
    port.parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "CARDBE_DEV_PORT must be a port between 1 and 65535",
            )
        })
}

#[cfg(all(debug_assertions, desktop))]
fn data_dir(_app: &tauri::App, port: u16) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri must have a project root")
        .join(".cardbe-debug")
        .join(port.to_string())
}

#[cfg(all(debug_assertions, mobile))]
fn data_dir(app: &tauri::App) -> PathBuf {
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data path")
        .join("debug-data")
}

#[cfg(not(debug_assertions))]
fn data_dir(app: &tauri::App) -> PathBuf {
    app.path()
        .app_local_data_dir()
        .expect("could not resolve app local data path")
        .join("data")
}

#[cfg(all(debug_assertions, desktop))]
struct DebugDataLock {
    _file: File,
}

#[cfg(all(debug_assertions, desktop))]
impl DebugDataLock {
    fn acquire(data_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(data_dir)?;
        let lock_path = data_dir.join(".lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("Debug data at {} is already in use", data_dir.display()),
            )),
            Err(TryLockError::Error(error)) => Err(error),
        }
    }
}

#[cfg(all(test, debug_assertions, desktop))]
mod debug_data_lock_tests {
    use super::DebugDataLock;
    use std::{fs, io, time::SystemTime};

    #[test]
    fn lock_prevents_a_second_user_until_released() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-debug-lock-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = DebugDataLock::acquire(&dir).unwrap();
        let error = DebugDataLock::acquire(&dir).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        drop(first);
        drop(DebugDataLock::acquire(&dir).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(all(debug_assertions, desktop))]
    let port = debug_port().expect("invalid CARDBE_DEV_PORT");
    #[allow(unused_mut)]
    let mut context = tauri::generate_context!();
    #[cfg(all(debug_assertions, desktop))]
    {
        let identifier = format!("{}.debug.p{port}", context.config().identifier);
        context.config_mut().identifier = identifier;
    }

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());

    #[cfg(desktop)]
    {
        builder = desktop::configure(builder);
        #[cfg(not(debug_assertions))]
        {
            builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                desktop::show_main_window(app)
            }));
        }
    }

    builder
        .setup(move |app| {
            #[cfg(all(debug_assertions, desktop))]
            let app_data_dir = data_dir(app, port);
            #[cfg(not(all(debug_assertions, desktop)))]
            let app_data_dir = data_dir(app);
            #[cfg(all(debug_assertions, desktop))]
            let debug_data_lock = DebugDataLock::acquire(&app_data_dir)?;
            let loaded = storage::load(&app_data_dir)?;
            #[cfg(all(debug_assertions, desktop))]
            app.manage(debug_data_lock);
            let iroh_database_path = loaded.database.path().to_path_buf();
            app.manage(Mutex::new(
                AppData::new(
                    loaded.stored,
                    loaded.database,
                    loaded.recovery_messages,
                    loaded.archives_loaded,
                )
                .map_err(std::io::Error::other)?,
            ));
            app.manage(share::LanShareState::default());
            app.manage(iroh_share::IrohShareState::default());
            let iroh_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let network = iroh_app.state::<iroh_share::IrohShareState>();
                iroh_share::restore_iroh_host(&network, iroh_database_path, iroh_app.clone()).await;
            });
            #[cfg(desktop)]
            desktop::setup(app)?;
            #[cfg(all(debug_assertions, desktop))]
            if let Some(window) = app.get_webview_window("main") {
                window.set_title(&format!("Cardbe [Debug: {port}]"))?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(desktop)]
            desktop::handle_window_event(window, event);
        })
        .invoke_handler(tauri::generate_handler![
            board::get_columns,
            board::get_board_columns,
            board::get_labels,
            boards::get_boards,
            boards::create_board,
            boards::rename_board,
            boards::switch_board,
            boards::delete_board,
            board::undo,
            board::add_column,
            board::update_column,
            board::move_column,
            board::delete_column,
            board::move_task,
            board::add_task,
            board::add_task_to_board,
            board::delete_task,
            board::update_task,
            archive::get_archives,
            archive::archive_task,
            archive::archive_all_tasks,
            archive::unarchive_task,
            calendar_export::get_calendar_pdf_font,
            import_export::export_data,
            import_export::import_data,
            import_export::import_board_as_new,
            import_export::export_all_boards,
            import_export::import_all_boards,
            iroh_share::create_iroh_invite,
            iroh_share::join_iroh_invite,
            iroh_share::sync_iroh_board,
            iroh_share::request_iroh_board_access,
            iroh_share::resolve_iroh_conflict,
            iroh_share::iroh_invite_qr_svg,
            iroh_share::list_iroh_invites,
            iroh_share::set_iroh_device_approved,
            iroh_share::iroh_host_error,
            iroh_share::ensure_iroh_host,
            iroh_share::get_iroh_invite_access,
            iroh_share::update_iroh_invite,
            iroh_share::delete_iroh_invite,
            notifications::check_expired_tasks,
            notifications::get_expired_tasks,
            notes::get_notes,
            notes::create_note,
            notes::create_quick_note,
            notes::update_note,
            notes::delete_note,
            settings::get_settings,
            settings::set_notify_enabled,
            settings::take_recovery_messages,
            share::publish_lan_share,
            share::revoke_lan_share,
            templates::get_task_templates,
            templates::save_task_template,
            templates::update_task_template,
            templates::delete_task_template,
            update::check_for_update,
            update::open_external_url,
            update::open_latest_release,
        ])
        .run(context)
        .expect("error while running tauri application");
}
