use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder, Window,
    WindowEvent,
};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::global_shortcuts::{self, ShortcutRegistry};
use crate::models::ShortcutAction;
use crate::state::{lock_app, SharedAppData};

const FLOATING_TASK_LABEL_PREFIX: &str = "float-task-";

fn run_shortcut_action<R: Runtime>(app: &AppHandle<R>, action: ShortcutAction) {
    match action {
        ShortcutAction::QuickTask => show_quick_add_window(app, "quick-task"),
        ShortcutAction::QuickNote => show_quick_add_window(app, "quick-note"),
        ShortcutAction::FloatingTasks => toggle_floating_task_windows(app),
        ShortcutAction::ShowCardbe => show_main_window(app),
    }
}

pub fn configure<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.manage(ShortcutRegistry::default()).plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, pressed, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                if let Some(action) = app.state::<ShortcutRegistry>().action_for(pressed) {
                    run_shortcut_action(app, action);
                }
            })
            .build(),
    )
}

pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let database_ready = app.try_state::<SharedAppData>().is_some();
    if database_ready {
        if let Ok(guard) = lock_app(&app.state::<SharedAppData>()) {
            app.state::<ShortcutRegistry>()
                .load(&guard.stored.settings.global_shortcuts);
        }
    }
    let shortcuts_enabled =
        database_ready && global_shortcuts_enabled(app.app_handle()).unwrap_or(true);
    if shortcuts_enabled {
        global_shortcuts::apply_enabled(app.app_handle(), true);
    }

    let registry = app.state::<ShortcutRegistry>();
    let quick_task = MenuItem::with_id(
        app,
        "quick-task",
        registry.menu_text(ShortcutAction::QuickTask),
        database_ready,
        None::<&str>,
    )?;
    let quick_note = MenuItem::with_id(
        app,
        "quick-note",
        registry.menu_text(ShortcutAction::QuickNote),
        database_ready,
        None::<&str>,
    )?;
    let floating_toggle = MenuItem::with_id(
        app,
        "toggle-floating-tasks",
        registry.menu_text(ShortcutAction::FloatingTasks),
        database_ready,
        None::<&str>,
    )?;
    let shortcut_toggle = CheckMenuItem::with_id(
        app,
        global_shortcuts::TOGGLE_MENU_ID,
        "Enable global shortcuts",
        database_ready,
        shortcuts_enabled,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(
        app,
        "show",
        registry.menu_text(ShortcutAction::ShowCardbe),
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Cardbe", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &quick_task,
            &quick_note,
            &floating_toggle,
            &shortcut_toggle,
            &show,
            &separator,
            &quit,
        ],
    )?;

    let shortcut_toggle_for_menu = shortcut_toggle.clone();
    let mut tray = TrayIconBuilder::with_id("cardbe")
        .tooltip("Cardbe")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quick-task" if database_ready => show_quick_add_window(app, "quick-task"),
            "quick-note" if database_ready => show_quick_add_window(app, "quick-note"),
            "toggle-floating-tasks" if database_ready => toggle_floating_task_windows(app),
            global_shortcuts::TOGGLE_MENU_ID if database_ready => {
                let Ok(was_enabled) = global_shortcuts_enabled(app) else {
                    return;
                };
                if let Err(error) = global_shortcuts::set_enabled(app, !was_enabled) {
                    log::error!(target: "desktop", "Could not save the global shortcut setting: {error:?}");
                    // The check item toggles itself when clicked.
                    if let Err(error) = shortcut_toggle_for_menu.set_checked(was_enabled) {
                        log::warn!(target: "desktop", "Could not restore the global shortcut menu state: {error}");
                    }
                }
            }
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(menu);
    Ok(())
}

pub fn set_menu_labels<R: Runtime>(app: &AppHandle<R>, labels: [String; 6]) -> Result<(), String> {
    let menu = app
        .try_state::<Menu<R>>()
        .ok_or("Desktop menu is not ready")?;
    let [quick_task, quick_note, floating_tasks, toggle_shortcuts, show, quit] = labels;
    app.state::<ShortcutRegistry>()
        .set_menu_labels([quick_task, quick_note, floating_tasks, show]);
    global_shortcuts::refresh_menu(app);
    for (id, text) in [
        (global_shortcuts::TOGGLE_MENU_ID, toggle_shortcuts),
        ("quit", quit),
    ] {
        if let Some(item) = menu.get(id) {
            if let Some(item) = item.as_menuitem() {
                item.set_text(&text).map_err(|error| error.to_string())?;
            } else if let Some(item) = item.as_check_menuitem() {
                item.set_text(&text).map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

fn global_shortcuts_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, String> {
    let state = app.state::<SharedAppData>();
    let guard = lock_app(&state).map_err(|error| {
        log::error!(target: "desktop", "Could not read the global shortcut setting: {error}");
        error.to_string()
    })?;
    Ok(guard.stored.settings.global_shortcuts_enabled)
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    // Tauri creates this window from `app.windows` in tauri.conf.json before
    // setup runs. It is the default visible window because `visible` is true.
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.unminimize() {
            log::warn!(target: "desktop", "Could not unminimize the main window: {error}");
        }
        if let Err(error) = window.show() {
            log::warn!(target: "desktop", "Could not show the main window: {error}");
        }
        if let Err(error) = window.set_focus() {
            log::warn!(target: "desktop", "Could not focus the main window: {error}");
        }
    }
}

fn show_quick_add_window<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        if let Err(error) = window.center() {
            log::warn!(target: "desktop", "Could not center the quick-add window: {error}");
        }
        if let Err(error) = window.show() {
            log::warn!(target: "desktop", "Could not show the quick-add window: {error}");
        }
        if let Err(error) = window.set_focus() {
            log::warn!(target: "desktop", "Could not focus the quick-add window: {error}");
        }
    }
}

fn floating_task_label(board_id: i64, task_id: i64) -> String {
    format!("{FLOATING_TASK_LABEL_PREFIX}{board_id}-{task_id}")
}

/// Opens one always-on-top reminder window per task, or focuses the existing
/// one. The window-state plugin restores its last position and size by label.
pub fn open_floating_task_window<R: Runtime>(
    app: &AppHandle<R>,
    board_id: i64,
    task_id: i64,
) -> tauri::Result<()> {
    let label = floating_task_label(board_id, task_id);
    if let Some(window) = app.get_webview_window(&label) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("float-task?board={board_id}&task={task_id}").into()),
    )
    .title("Cardbe")
    .inner_size(320.0, 220.0)
    .min_inner_size(220.0, 32.0)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .maximizable(false)
    .minimizable(false)
    .build()
    .map(|_| ())
}

/// Hides every floating task while any is visible; otherwise shows them all.
/// Hidden windows stay open, so they are still restored on the next launch.
fn toggle_floating_task_windows<R: Runtime>(app: &AppHandle<R>) {
    let windows: Vec<_> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with(FLOATING_TASK_LABEL_PREFIX))
        .map(|(_, window)| window)
        .collect();
    let hide = windows
        .iter()
        .any(|window| window.is_visible().unwrap_or(false));
    for window in windows {
        let result = if hide { window.hide() } else { window.show() };
        if let Err(error) = result {
            log::warn!(target: "desktop", "Could not toggle a floating task window: {error}");
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenTaskRequest {
    board_id: i64,
    task_id: i64,
}

pub fn open_task_in_main_window<R: Runtime>(
    app: &AppHandle<R>,
    board_id: i64,
    task_id: i64,
) -> tauri::Result<()> {
    show_main_window(app);
    app.emit_to(
        "main",
        "cardbe:open-task",
        OpenTaskRequest { board_id, task_id },
    )
}

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    match event {
        // Floating reminders close for real; the main and quick-add windows
        // stay alive in the background.
        WindowEvent::CloseRequested { .. }
            if window.label().starts_with(FLOATING_TASK_LABEL_PREFIX) => {}
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if let Err(error) = window.hide() {
                log::warn!(target: "desktop", "Could not hide the main window: {error}");
            }
        }
        WindowEvent::Focused(false) if window.label().starts_with("quick-") => {
            if let Err(error) = window.hide() {
                log::warn!(target: "desktop", "Could not hide the quick-add window: {error}");
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn floating_task_labels_match_the_capability_pattern() {
        assert_eq!(super::floating_task_label(12, 34), "float-task-12-34");
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert!(capability["windows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|window| window == "float-task-*"));
    }

    #[test]
    fn quick_add_windows_are_precreated_and_hidden() {
        let config: tauri::Config =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let main = config
            .app
            .windows
            .iter()
            .find(|window| window.label == "main")
            .unwrap();
        assert!(main.create);
        for label in ["quick-task", "quick-note"] {
            let window = config
                .app
                .windows
                .iter()
                .find(|window| window.label == label)
                .unwrap();
            assert!(window.create);
            assert!(!window.visible);
            assert!(window.always_on_top && window.skip_taskbar);
            assert!(!window.resizable && !window.decorations);
        }
    }
}
