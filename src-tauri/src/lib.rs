mod commands;
#[cfg(desktop)]
mod desktop;
mod models;
mod state;
mod storage;

use commands::{
    archive, board, boards, calendar_export, import_export, notes, notifications, settings, share,
    templates, update,
};
use state::AppData;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());

    #[cfg(desktop)]
    {
        builder = desktop::configure(builder).plugin(tauri_plugin_single_instance::init(
            |app, _args, _cwd| desktop::show_main_window(app),
        ));
    }

    builder
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_local_data_dir()
                .expect("could not resolve app local data path")
                .join("data");
            let loaded = storage::load(&app_data_dir)?;
            app.manage(Mutex::new(AppData::new(
                loaded.stored,
                loaded.database,
                loaded.recovery_messages,
                loaded.archives_loaded,
            )));
            app.manage(share::LanShareState::default());
            #[cfg(desktop)]
            desktop::setup(app)?;
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
