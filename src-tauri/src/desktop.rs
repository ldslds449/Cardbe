use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, Runtime, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::state::{update_stored, SharedAppData};

const QUICK_TASK_SHORTCUT: &str = "Ctrl+Alt+T";
const QUICK_NOTE_SHORTCUT: &str = "Ctrl+Alt+N";
const SHOW_CARDBE_SHORTCUT: &str = "Ctrl+Alt+C";

fn quick_task_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyT)
}

fn quick_note_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyN)
}

fn show_cardbe_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyC)
}

pub fn configure<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    let task_shortcut = quick_task_shortcut();
    let note_shortcut = quick_note_shortcut();
    let show_shortcut = show_cardbe_shortcut();
    builder.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, pressed, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                if pressed == &task_shortcut {
                    show_quick_add_window(app, "quick-task");
                } else if pressed == &note_shortcut {
                    show_quick_add_window(app, "quick-note");
                } else if pressed == &show_shortcut {
                    show_main_window(app);
                }
            })
            .build(),
    )
}

pub fn setup<R: Runtime>(app: &mut App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let shortcuts_enabled = global_shortcuts_enabled(app.app_handle()).unwrap_or(true);
    if shortcuts_enabled {
        if let Err(error) = set_global_shortcuts_enabled(app.app_handle(), true) {
            eprintln!("Could not enable Cardbe global shortcuts: {error}");
        }
    }

    let quick_task = MenuItem::with_id(
        app,
        "quick-task",
        format!("New Task\t{QUICK_TASK_SHORTCUT}"),
        true,
        None::<&str>,
    )?;
    let quick_note = MenuItem::with_id(
        app,
        "quick-note",
        format!("New Note\t{QUICK_NOTE_SHORTCUT}"),
        true,
        None::<&str>,
    )?;
    let shortcut_toggle = CheckMenuItem::with_id(
        app,
        "toggle-shortcuts",
        "Enable global shortcuts",
        true,
        shortcuts_enabled,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(
        app,
        "show",
        format!("Show Cardbe\t{SHOW_CARDBE_SHORTCUT}"),
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
            "quick-task" => show_quick_add_window(app, "quick-task"),
            "quick-note" => show_quick_add_window(app, "quick-note"),
            "toggle-shortcuts" => {
                let was_enabled = global_shortcuts_enabled(app).unwrap_or(true);
                let enabled = !was_enabled;
                if let Err(error) = set_global_shortcuts_enabled(app, enabled) {
                    eprintln!("Could not change Cardbe global shortcuts: {error}");
                    let _ = shortcut_toggle_for_menu.set_checked(was_enabled);
                    return;
                }

                let state = app.state::<SharedAppData>();
                if let Err(error) = update_stored(&state, |data| {
                    data.settings.global_shortcuts_enabled = enabled;
                    Ok(())
                }) {
                    eprintln!("Could not save the global shortcut setting: {error}");
                    let _ = set_global_shortcuts_enabled(app, was_enabled);
                    let _ = shortcut_toggle_for_menu.set_checked(was_enabled);
                    return;
                }
                let _ = shortcut_toggle_for_menu.set_checked(enabled);
            }
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn global_shortcuts_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, String> {
    let state = app.state::<SharedAppData>();
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    Ok(guard.stored.settings.global_shortcuts_enabled)
}

fn set_global_shortcuts_enabled<R: Runtime>(
    app: &AppHandle<R>,
    enabled: bool,
) -> Result<(), String> {
    let manager = app.global_shortcut();
    let shortcuts = [
        quick_task_shortcut(),
        quick_note_shortcut(),
        show_cardbe_shortcut(),
    ];
    let previous_registration = shortcuts.map(|shortcut| manager.is_registered(shortcut));

    let result: Result<(), String> = (|| {
        for (shortcut, was_registered) in shortcuts
            .iter()
            .copied()
            .zip(previous_registration.into_iter())
        {
            if enabled && !was_registered {
                manager
                    .register(shortcut)
                    .map_err(|error| error.to_string())?;
            }
            if !enabled && was_registered {
                manager
                    .unregister(shortcut)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    })();

    if result.is_err() {
        for (shortcut, was_registered) in shortcuts
            .iter()
            .copied()
            .zip(previous_registration.into_iter())
        {
            if was_registered {
                let _ = manager.register(shortcut);
            } else {
                let _ = manager.unregister(shortcut);
            }
        }
    }
    result
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    // Tauri creates this window from `app.windows` in tauri.conf.json before
    // setup runs. It is the default visible window because `visible` is true.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn show_quick_add_window<R: Runtime>(app: &AppHandle<R>, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
        }
        WindowEvent::Focused(false) if window.label().starts_with("quick-") => {
            let _ = window.hide();
        }
        _ => {}
    }
}
