mod commands;
#[cfg(desktop)]
mod desktop;
mod errors;
mod link_preview;
mod loro_board;
mod models;
mod plugins;
mod search;
mod state;
mod storage;

use commands::{
    archive, board, boards, calendar_export, diagnostics, import_export, iroh_share, notes,
    notifications, settings, share, task_explorer, templates, update,
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
use tokio::sync::watch;

struct StartupError {
    sender: watch::Sender<Option<Result<(), String>>>,
    receiver: watch::Receiver<Option<Result<(), String>>>,
}

impl StartupError {
    async fn wait(&self) -> Result<Option<errors::CommandError>, errors::CommandError> {
        let mut status = self.receiver.clone();
        loop {
            let current = status.borrow().clone();
            match current {
                Some(Ok(())) => return Ok(None),
                Some(Err(error)) => return Ok(Some(errors::CommandError::internal(error))),
                None => status
                    .changed()
                    .await
                    .map_err(|_| errors::CommandError::internal("Startup status unavailable"))?,
            }
        }
    }
}

#[tauri::command]
async fn get_startup_error(
    state: tauri::State<'_, StartupError>,
) -> Result<Option<errors::CommandError>, errors::CommandError> {
    state.wait().await
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[tokio::test]
    async fn startup_waits_for_completion_and_preserves_storage_failure() {
        let (sender, receiver) = watch::channel(None);
        let state = StartupError { sender, receiver };
        let waiting = state.wait();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut waiting)
                .await
                .is_err()
        );
        state.sender.send(Some(Ok(()))).unwrap();
        assert!(waiting.await.unwrap().is_none());
        state
            .sender
            .send(Some(Err("storage failed".into())))
            .unwrap();
        assert!(matches!(
            state.wait().await.unwrap(),
            Some(errors::CommandError::InternalError)
        ));
    }
}

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
fn debug_instance_id(port: u16) -> io::Result<String> {
    match std::env::var("CARDBE_DEV_INSTANCE_ID") {
        Ok(id) if id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()) => Ok(id),
        Err(std::env::VarError::NotPresent) => Ok(port.to_string()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid CARDBE_DEV_INSTANCE_ID",
        )),
    }
}

#[cfg(all(debug_assertions, desktop))]
fn data_dir(instance_id: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri must have a project root")
        .join(".cardbe-debug")
        .join(instance_id)
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
            .truncate(false)
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
    fn migrate(source: &Path, destination: &Path) -> io::Result<Self> {
        // A completed migration always reuses the ID directory, even if an older
        // desktop later creates another numeric directory.
        if destination.exists() {
            return Self::acquire(destination);
        }
        if !source.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Legacy development data is missing",
            ));
        }
        let source_lock = Self::acquire(source)?;
        // The launcher owns this ID. On Windows other open runtime handles also
        // prevent the rename; Unix keeps the source lock through the rename.
        #[cfg(windows)]
        {
            drop(source_lock);
            fs::rename(source, destination)?;
            Self::acquire(destination)
        }
        #[cfg(not(windows))]
        {
            fs::rename(source, destination)?;
            Ok(source_lock)
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
        let destination = dir.with_extension("migrated");
        fs::write(dir.join("identity"), "saved-device").unwrap();
        assert!(DebugDataLock::migrate(&dir, &destination).is_err());
        assert!(!destination.exists());
        drop(first);
        let migrated = DebugDataLock::migrate(&dir, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination.join("identity")).unwrap(),
            "saved-device"
        );
        assert!(!dir.exists());
        assert!(DebugDataLock::acquire(&destination).is_err());
        drop(migrated);
        drop(DebugDataLock::migrate(&dir, &destination).unwrap());
        fs::remove_dir_all(destination).unwrap();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::install_panic_logging();
    #[cfg(all(debug_assertions, desktop))]
    let port = debug_port().expect("invalid CARDBE_DEV_PORT");
    #[cfg(all(debug_assertions, desktop))]
    let instance_id = debug_instance_id(port).expect("invalid development instance ID");
    #[cfg(all(debug_assertions, desktop))]
    let _instance_guard = match std::env::var("CARDBE_DEV_INSTANCE_GUARD_PORT") {
        Ok(value) => {
            let port = value
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .expect("invalid development instance guard port");
            Some(
                std::net::TcpListener::bind(("127.0.0.1", port))
                    .expect("development instance is already in use"),
            )
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid development instance guard: {error}"),
    };
    // Track ACL inputs read by generate_context! so compiler caches invalidate permission changes.
    const _: &str = include_str!(concat!(env!("OUT_DIR"), "/capabilities.json"));
    const _: &str = include_str!(concat!(env!("OUT_DIR"), "/acl-manifests.json"));
    #[allow(unused_mut)]
    let mut context = tauri::generate_context!();
    #[cfg(all(debug_assertions, desktop))]
    {
        let identifier = format!("{}.debug.p{instance_id}", context.config().identifier);
        context.config_mut().identifier = identifier;
    }

    let (startup_sender, startup_receiver) = watch::channel::<Option<Result<(), String>>>(None);
    let mut builder = tauri::Builder::default()
        .manage(StartupError {
            sender: startup_sender,
            receiver: startup_receiver,
        })
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .max_file_size(5_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());

    #[cfg(desktop)]
    {
        builder = desktop::configure(builder);
        builder = builder.plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .with_denylist(&["quick-task", "quick-note"])
                .build(),
        );
        #[cfg(not(debug_assertions))]
        {
            builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                desktop::show_main_window(app)
            }));
        }
    }

    builder
        .setup(move |app| {
            log::info!(
                target: "startup",
                "Starting Cardbe {} on {} {}",
                app.package_info().version,
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            #[cfg(all(debug_assertions, desktop))]
            let app_data_dir = data_dir(&instance_id);
            #[cfg(not(all(debug_assertions, desktop)))]
            let app_data_dir = data_dir(app);
            #[cfg(all(debug_assertions, desktop))]
            let debug_data_lock = match std::env::var("CARDBE_DEV_LEGACY_PORT") {
                Ok(value) => {
                    let port = value
                        .parse::<u16>()
                        .ok()
                        .filter(|port| *port != 0)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "Invalid legacy development port",
                            )
                        })?;
                    DebugDataLock::migrate(&data_dir(&port.to_string()), &app_data_dir)?
                }
                Err(std::env::VarError::NotPresent) => DebugDataLock::acquire(&app_data_dir)?,
                Err(error) => return Err(Box::new(error)),
            };
            #[cfg(all(debug_assertions, desktop))]
            app.manage(debug_data_lock);
            app.manage(share::LanShareState::default());
            app.manage(iroh_share::IrohShareState::default());
            let startup_result = storage::load(&app_data_dir)
                .map_err(|error| {
                    format!(
                        "{error}\n\nDatabase: {}",
                        app_data_dir.join("data.sqlite3").display()
                    )
                })
                .and_then(|loaded| {
                    for message in &loaded.recovery_messages {
                        log::warn!(target: "storage", "Application data recovery: {message}");
                    }
                    let database_path = loaded.database.path().to_path_buf();
                    let data = AppData::new(
                        loaded.stored,
                        loaded.database,
                        loaded.recovery_messages,
                        loaded.archives_loaded,
                    )
                    .map_err(|error| format!("{error}\n\nDatabase: {}", database_path.display()))?;
                    Ok((data, database_path))
                });
            let startup_status = match startup_result {
                Ok((data, database_path)) => {
                    app.manage(Mutex::new(data));
                    app.manage(plugins::manager::PluginManager::default());
                    match plugins::manager::initialize(app.handle()) {
                        Ok(()) => plugins::manager::start_scheduler(app.handle().clone()),
                        Err(_) => {
                            app.state::<plugins::manager::PluginManager>().safe_mode.store(true, std::sync::atomic::Ordering::SeqCst);
                            log::warn!(target: "plugins", "Plugin initialization failed; plugins disabled for this session");
                        }
                    }
                    let iroh_app = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let network = iroh_app.state::<iroh_share::IrohShareState>();
                        iroh_share::restore_iroh_host(&network, database_path, iroh_app.clone())
                            .await;
                    });
                    Ok(())
                }
                Err(error) => {
                    log::error!(target: "storage", "Could not load application data: {error}");
                    Err(error)
                }
            };
            #[cfg(desktop)]
            desktop::setup(app)?;
            #[cfg(all(debug_assertions, desktop))]
            if let Some(window) = app.get_webview_window("main") {
                window.set_title(&format!("Cardbe [Debug: {port}]"))?;
            }
            let _ = app
                .state::<StartupError>()
                .sender
                .send(Some(startup_status));
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(desktop)]
            desktop::handle_window_event(window, event);
        })
        .invoke_handler(tauri::generate_handler![
            commands::plugins::get_plugin_state,
            commands::plugins::install_plugin_package,
            commands::plugins::remove_plugin_package,
            commands::plugins::save_plugin_instance,
            commands::plugins::remove_plugin_instance,
            commands::plugins::set_plugin_enabled,
            commands::plugins::run_plugin_instance,
            commands::plugins::cancel_plugin_run,
            commands::plugins::get_plugin_runs,
            commands::plugins::set_plugin_safe_mode,
            commands::link_preview::get_link_preview,
            commands::link_preview::get_link_preview_image,
            board::get_columns,
            board::get_board_columns,
            board::add_column_to_board,
            board::search_tasks,
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
            board::move_task_to_board,
            board::add_task,
            board::add_task_to_board,
            board::delete_task,
            board::update_task,
            archive::get_archives,
            archive::list_archives,
            archive::get_archive_task,
            archive::archive_task,
            archive::archive_all_tasks,
            archive::unarchive_task,
            calendar_export::get_calendar_pdf_font,
            diagnostics::log_frontend,
            diagnostics::get_diagnostics,
            diagnostics::open_log_folder,
            diagnostics::export_debug_information,
            import_export::export_data,
            import_export::import_data,
            import_export::import_board_as_new,
            import_export::export_all_boards,
            import_export::import_all_boards,
            import_export::validate_all_boards_backup,
            iroh_share::create_iroh_invite,
            iroh_share::join_iroh_invite,
            iroh_share::sync_iroh_board,
            iroh_share::request_iroh_board_access,
            iroh_share::resolve_iroh_conflict,
            iroh_share::iroh_invite_qr_svg,
            iroh_share::list_iroh_invites,
            iroh_share::set_iroh_device_approved,
            iroh_share::iroh_host_error,
            iroh_share::get_iroh_connection_details,
            iroh_share::ensure_iroh_host,
            iroh_share::get_iroh_invite_access,
            iroh_share::update_iroh_invite,
            iroh_share::delete_iroh_invite,
            notifications::check_expired_tasks,
            notifications::get_expired_tasks,
            notifications::list_expired_tasks,
            notifications::get_task_detail,
            task_explorer::list_all_tasks,
            notes::get_notes,
            notes::create_note,
            notes::create_quick_note,
            notes::update_note,
            notes::delete_note,
            settings::get_settings,
            settings::set_notify_enabled,
            settings::get_iroh_network_settings,
            settings::set_iroh_network_settings,
            settings::set_language,
            settings::set_theme,
            settings::set_desktop_menu_labels,
            settings::take_recovery_messages,
            get_startup_error,
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
