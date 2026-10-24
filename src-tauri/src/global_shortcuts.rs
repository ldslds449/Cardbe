//! Configurable, device-local global shortcuts.
//!
//! Each action registers independently: a combination owned by another app
//! must not stop the remaining Cardbe shortcuts from working.

use std::sync::Mutex;

use tauri::{menu::Menu, AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::commands::settings::{GlobalShortcutOverview, GlobalShortcutStatus};
use crate::errors::CommandError;
use crate::models::{GlobalShortcutSettings, ShortcutAction};
use crate::state::{lock_app, update_stored, SharedAppData};

type Binding = Option<(Shortcut, String)>;

/// Tells open Settings views that the shortcut state changed elsewhere, such as
/// from the tray menu.
pub const CHANGED_EVENT: &str = "cardbe:global-shortcuts-changed";
pub const TOGGLE_MENU_ID: &str = "toggle-shortcuts";

/// Parses a shortcut into its canonical `Ctrl+Alt+Shift+K` form. An empty
/// value turns the shortcut off. Ctrl or Alt is required so that a shortcut
/// never swallows ordinary typing.
fn parse_shortcut(value: &str) -> Result<Binding, ()> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    let mut key: Option<String> = None;
    for token in value.split('+').map(str::trim) {
        let modifier = match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => &mut ctrl,
            "alt" => &mut alt,
            "shift" => &mut shift,
            _ => {
                if key.is_some() || !valid_key(token) {
                    return Err(());
                }
                key = Some(token.to_ascii_uppercase());
                continue;
            }
        };
        if *modifier {
            return Err(());
        }
        *modifier = true;
    }
    let key = key.ok_or(())?;
    if !(ctrl || alt) {
        return Err(());
    }
    let canonical = [
        ctrl.then_some("Ctrl"),
        alt.then_some("Alt"),
        shift.then_some("Shift"),
        Some(key.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("+");
    let shortcut: Shortcut = canonical.parse().map_err(|_| ())?;
    Ok(Some((shortcut, canonical)))
}

fn valid_key(token: &str) -> bool {
    let upper = token.to_ascii_uppercase();
    if upper.len() == 1 {
        return upper
            .chars()
            .all(|character| character.is_ascii_alphanumeric());
    }
    upper.strip_prefix('F').is_some_and(|number| {
        !number.starts_with('0') && number.parse::<u8>().is_ok_and(|n| (1..=24).contains(&n))
    })
}

fn default_binding(action: ShortcutAction) -> Binding {
    parse_shortcut(action.default_shortcut()).ok().flatten()
}

/// Resolves stored settings. Invalid values fall back to the default, and a
/// combination already taken by an earlier action is turned off.
fn configured_bindings(settings: &GlobalShortcutSettings) -> [Binding; 4] {
    let mut bindings: [Binding; 4] = Default::default();
    for (index, action) in ShortcutAction::ALL.into_iter().enumerate() {
        let binding = match action.setting(settings) {
            None => default_binding(action),
            Some(value) => parse_shortcut(value).unwrap_or_else(|()| {
                log::warn!(target: "desktop", "Ignoring an invalid {action:?} shortcut setting");
                default_binding(action)
            }),
        };
        let duplicate = binding.as_ref().is_some_and(|(shortcut, _)| {
            bindings[..index]
                .iter()
                .flatten()
                .any(|(other, _)| other == shortcut)
        });
        if duplicate {
            log::warn!(target: "desktop", "Turning off a duplicate {action:?} shortcut");
        } else {
            bindings[index] = binding;
        }
    }
    bindings
}

fn index(action: ShortcutAction) -> usize {
    ShortcutAction::ALL
        .iter()
        .position(|candidate| *candidate == action)
        .unwrap_or_default()
}

struct RegistryState {
    bindings: [Binding; 4],
    menu_labels: [String; 4],
}

pub struct ShortcutRegistry(Mutex<RegistryState>);

impl Default for ShortcutRegistry {
    fn default() -> Self {
        Self(Mutex::new(RegistryState {
            bindings: configured_bindings(&GlobalShortcutSettings::default()),
            menu_labels: [
                "New Task".into(),
                "New Note".into(),
                "Show/Hide Floating Windows".into(),
                "Show Cardbe".into(),
            ],
        }))
    }
}

impl ShortcutRegistry {
    fn bindings(&self) -> [Binding; 4] {
        self.0
            .lock()
            .map(|state| state.bindings.clone())
            .unwrap_or_default()
    }

    pub fn load(&self, settings: &GlobalShortcutSettings) {
        if let Ok(mut state) = self.0.lock() {
            state.bindings = configured_bindings(settings);
        }
    }

    pub fn action_for(&self, pressed: &Shortcut) -> Option<ShortcutAction> {
        let bindings = self.bindings();
        ShortcutAction::ALL
            .into_iter()
            .zip(bindings)
            .find_map(|(action, binding)| {
                binding
                    .is_some_and(|(shortcut, _)| &shortcut == pressed)
                    .then_some(action)
            })
    }

    pub fn set_menu_labels(&self, labels: [String; 4]) {
        if let Ok(mut state) = self.0.lock() {
            state.menu_labels = labels;
        }
    }

    pub fn menu_text(&self, action: ShortcutAction) -> String {
        let Ok(state) = self.0.lock() else {
            return action.default_shortcut().into();
        };
        let label = &state.menu_labels[index(action)];
        match &state.bindings[index(action)] {
            Some((_, text)) => format!("{label}\t{text}"),
            None => label.clone(),
        }
    }
}

pub fn menu_id(action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::QuickTask => "quick-task",
        ShortcutAction::QuickNote => "quick-note",
        ShortcutAction::FloatingTasks => "toggle-floating-tasks",
        ShortcutAction::ShowCardbe => "show",
    }
}

pub fn refresh_menu<R: Runtime>(app: &AppHandle<R>) {
    let Some(menu) = app.try_state::<Menu<R>>() else {
        return;
    };
    let registry = app.state::<ShortcutRegistry>();
    for action in ShortcutAction::ALL {
        if let Some(item) = menu
            .get(menu_id(action))
            .and_then(|item| item.as_menuitem().cloned())
        {
            if let Err(error) = item.set_text(registry.menu_text(action)) {
                log::warn!(target: "desktop", "Could not update a shortcut menu item: {error}");
            }
        }
    }
}

/// Registers or unregisters every configured shortcut independently.
pub fn apply_enabled<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    let manager = app.global_shortcut();
    for (shortcut, _) in app
        .state::<ShortcutRegistry>()
        .bindings()
        .into_iter()
        .flatten()
    {
        let change = match (enabled, manager.is_registered(shortcut)) {
            (true, false) => manager.register(shortcut),
            (false, true) => manager.unregister(shortcut),
            _ => Ok(()),
        };
        if let Err(error) = change {
            log::warn!(target: "desktop", "Could not update a global shortcut: {error}");
        }
    }
}

fn statuses<R: Runtime>(app: &AppHandle<R>) -> Vec<GlobalShortcutStatus> {
    let manager = app.global_shortcut();
    ShortcutAction::ALL
        .into_iter()
        .zip(app.state::<ShortcutRegistry>().bindings())
        .map(|(action, binding)| GlobalShortcutStatus {
            action,
            default_shortcut: action.default_shortcut(),
            active: binding
                .as_ref()
                .is_some_and(|(shortcut, _)| manager.is_registered(*shortcut)),
            shortcut: binding.map(|(_, text)| text),
        })
        .collect()
}

pub fn overview<R: Runtime>(app: &AppHandle<R>) -> Result<GlobalShortcutOverview, CommandError> {
    Ok(GlobalShortcutOverview {
        enabled: global_shortcuts_enabled(app)?,
        shortcuts: statuses(app),
    })
}

fn global_shortcuts_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, CommandError> {
    let state = app.state::<SharedAppData>();
    let guard = lock_app(&state)?;
    Ok(guard.stored.settings.global_shortcuts_enabled)
}

fn notify_changed<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.emit(CHANGED_EVENT, ()) {
        log::warn!(target: "desktop", "Could not announce a global shortcut change: {error}");
    }
}

/// Turns the whole group on or off from Settings or the tray menu.
/// Unavailable combinations are reported per shortcut, so turning the group
/// on succeeds even when another app owns some of the keys.
pub fn set_enabled<R: Runtime>(
    app: &AppHandle<R>,
    enabled: bool,
) -> Result<GlobalShortcutOverview, CommandError> {
    let was_enabled = global_shortcuts_enabled(app)?;
    if was_enabled != enabled {
        apply_enabled(app, enabled);
        if let Err(error) = update_stored(&app.state::<SharedAppData>(), |data| {
            data.settings.global_shortcuts_enabled = enabled;
            Ok(())
        }) {
            apply_enabled(app, was_enabled);
            return Err(error.into());
        }
    }
    if let Some(item) = app
        .try_state::<Menu<R>>()
        .and_then(|menu| menu.get(TOGGLE_MENU_ID))
        .and_then(|item| item.as_check_menuitem().cloned())
    {
        if let Err(error) = item.set_checked(enabled) {
            log::warn!(target: "desktop", "Could not update the global shortcut menu state: {error}");
        }
    }
    notify_changed(app);
    overview(app)
}

/// Registers the new combination before saving it, so an unavailable key
/// leaves the previous shortcut and setting untouched. With `take_over`, a
/// combination used by another action moves here and that action turns off.
pub fn set<R: Runtime>(
    app: &AppHandle<R>,
    action: ShortcutAction,
    value: &str,
    take_over: bool,
) -> Result<GlobalShortcutOverview, CommandError> {
    let next = parse_shortcut(value).map_err(|()| CommandError::InvalidArgument)?;
    let registry = app.state::<ShortcutRegistry>();
    let bindings = registry.bindings();
    let next_shortcut = next.as_ref().map(|(shortcut, _)| *shortcut);
    let owner = next_shortcut.and_then(|next| {
        ShortcutAction::ALL
            .into_iter()
            .zip(&bindings)
            .find_map(|(other, binding)| {
                (other != action
                    && binding
                        .as_ref()
                        .is_some_and(|(shortcut, _)| *shortcut == next))
                .then_some(other)
            })
    });
    if owner.is_some() && !take_over {
        return Err(CommandError::ShortcutAlreadyAssigned);
    }
    let previous = bindings[index(action)]
        .as_ref()
        .map(|(shortcut, _)| *shortcut);
    let manager = app.global_shortcut();

    let mut unregistered_previous = false;
    let mut registered_next = false;
    if global_shortcuts_enabled(app)? {
        if let Some(previous) = previous
            .filter(|previous| Some(*previous) != next_shortcut && manager.is_registered(*previous))
        {
            manager
                .unregister(previous)
                .map_err(CommandError::internal)?;
            unregistered_previous = true;
        }
        if let Some(next) = next_shortcut.filter(|next| !manager.is_registered(*next)) {
            if manager.register(next).is_err() {
                if let Some(previous) = previous.filter(|_| unregistered_previous) {
                    if let Err(error) = manager.register(previous) {
                        log::warn!(target: "desktop", "Could not restore a global shortcut: {error}");
                    }
                }
                return Err(CommandError::ShortcutUnavailable);
            }
            registered_next = true;
        }
    }

    let stored = next
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    if let Err(error) = update_stored(&app.state::<SharedAppData>(), |data| {
        *action.setting_mut(&mut data.settings.global_shortcuts) = Some(stored);
        if let Some(owner) = owner {
            *owner.setting_mut(&mut data.settings.global_shortcuts) = Some(String::new());
        }
        Ok(())
    }) {
        if let Some(next) = next_shortcut.filter(|_| registered_next) {
            let _ = manager.unregister(next);
        }
        if let Some(previous) = previous.filter(|_| unregistered_previous) {
            let _ = manager.register(previous);
        }
        return Err(error.into());
    }

    if let Ok(mut state) = registry.0.lock() {
        state.bindings[index(action)] = next;
        if let Some(owner) = owner {
            state.bindings[index(owner)] = None;
        }
    }
    refresh_menu(app);
    notify_changed(app);
    overview(app)
}

#[cfg(test)]
mod tests {
    use super::{configured_bindings, parse_shortcut};
    use crate::models::{GlobalShortcutSettings, ShortcutAction};

    fn texts(settings: &GlobalShortcutSettings) -> Vec<Option<String>> {
        configured_bindings(settings)
            .into_iter()
            .map(|binding| binding.map(|(_, text)| text))
            .collect()
    }

    #[test]
    fn shortcuts_are_canonical_and_restricted() {
        let canonical = |value: &str| parse_shortcut(value).unwrap().unwrap().1;
        assert_eq!(canonical("shift + alt + ctrl + f"), "Ctrl+Alt+Shift+F");
        assert_eq!(canonical("Control+Alt+7"), "Ctrl+Alt+7");
        assert_eq!(canonical("Alt+F12"), "Alt+F12");
        assert_eq!(canonical("Ctrl+Shift+F24"), "Ctrl+Shift+F24");
        assert_eq!(parse_shortcut("  "), Ok(None));
        for invalid in [
            "F",
            "Shift+F",
            "Ctrl+Alt",
            "Ctrl+Ctrl+F",
            "Ctrl+Alt+F+G",
            "Ctrl+Alt+F25",
            "Ctrl+Alt+F01",
            "Ctrl+Alt+Space",
            "Super+F",
        ] {
            assert!(parse_shortcut(invalid).is_err(), "{invalid}");
        }
        for action in ShortcutAction::ALL {
            assert!(parse_shortcut(action.default_shortcut()).unwrap().is_some());
        }
    }

    #[test]
    fn stored_settings_resolve_defaults_overrides_and_conflicts() {
        assert_eq!(
            texts(&GlobalShortcutSettings::default()),
            ["Ctrl+Alt+T", "Ctrl+Alt+N", "Ctrl+Alt+F", "Ctrl+Alt+C"].map(|text| Some(text.into()))
        );
        let custom = GlobalShortcutSettings {
            quick_task: Some("Ctrl+Shift+K".into()),
            quick_note: Some(String::new()),
            floating_tasks: Some("not a shortcut".into()),
            // Collides with the custom quick-task binding, so it is turned off.
            show_cardbe: Some("ctrl+shift+k".into()),
        };
        assert_eq!(
            texts(&custom),
            [
                Some("Ctrl+Shift+K".into()),
                None,
                Some("Ctrl+Alt+F".into()),
                None
            ]
        );
    }

    #[test]
    fn shortcut_settings_are_optional_in_stored_settings() {
        let legacy: crate::models::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.global_shortcuts, GlobalShortcutSettings::default());
        let mut settings = legacy;
        settings.global_shortcuts.quick_note = Some(String::new());
        settings.global_shortcuts.floating_tasks = Some("Alt+F9".into());
        let restored: crate::models::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored, settings);
    }
}
