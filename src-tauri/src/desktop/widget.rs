//! Device-local desktop widgets, separate from floating reminder windows.
use crate::errors::CommandError;
#[cfg(windows)]
use tauri::Manager;

#[tauri::command]
pub fn desktop_widget_supported() -> bool {
    cfg!(windows)
}

#[derive(serde::Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WidgetRef {
    Task { board_id: i64, task_id: i64 },
    Focus { id: String },
}

/// Where a widget is shown: attached to the desktop, or as a floating window.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    Desktop,
    Floating,
}

impl WidgetRef {
    fn label(&self, placement: &Placement) -> String {
        match (self, placement) {
            (Self::Task { board_id, task_id }, Placement::Desktop) => {
                format!("widget-task-{board_id}-{task_id}")
            }
            (Self::Focus { id }, Placement::Desktop) => {
                format!("widget-focus-{}", super::hex_id(id))
            }
            (Self::Task { board_id, task_id }, Placement::Floating) => {
                super::floating_task_label(*board_id, *task_id)
            }
            (Self::Focus { id }, Placement::Floating) => super::floating_focus_label(id),
        }
    }
}

#[tauri::command]
pub async fn remove_widget(
    app: tauri::AppHandle,
    widget: WidgetRef,
    placement: Placement,
) -> Result<(), CommandError> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window(&widget.label(&placement)) {
        window.destroy().map_err(CommandError::internal)?;
    }
    Ok(())
}

/// Whether each floating window is open and visible; the tray can hide them.
#[tauri::command]
pub fn floating_windows_visible(app: tauri::AppHandle, widgets: Vec<WidgetRef>) -> Vec<bool> {
    use tauri::Manager;
    widgets
        .iter()
        .map(|widget| {
            app.get_webview_window(&widget.label(&Placement::Floating))
                .is_some_and(|window| window.is_visible().unwrap_or(false))
        })
        .collect()
}

/// Hides one floating window, keeping it open like the tray toggle does.
#[tauri::command]
pub async fn hide_floating_window(
    app: tauri::AppHandle,
    widget: WidgetRef,
) -> Result<(), CommandError> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window(&widget.label(&Placement::Floating)) {
        window.hide().map_err(CommandError::internal)?;
        super::set_low_memory(&window, true);
    }
    Ok(())
}

#[tauri::command]
pub async fn open_desktop_widget_manager(app: tauri::AppHandle) -> Result<(), CommandError> {
    use tauri::Emitter;
    super::show_main_window(&app);
    app.emit_to("main", "cardbe:open-widget-manager", ())
        .map_err(CommandError::internal)
}

pub async fn open_task(app: tauri::AppHandle, board: i64, task: i64) -> Result<(), CommandError> {
    open(
        app,
        format!("widget-task-{board}-{task}"),
        format!("float-task?board={board}&task={task}&widget=1"),
        (360.0, 200.0),
    )
    .await
}

pub async fn open_focus(app: tauri::AppHandle, id: &str) -> Result<(), CommandError> {
    let id = super::hex_id(id);
    open(
        app,
        format!("widget-focus-{id}"),
        format!("float-focus?focus={id}&widget=1"),
        (360.0, 400.0),
    )
    .await
}

async fn open(
    app: tauri::AppHandle,
    label: String,
    url: String,
    (width, height): (f64, f64),
) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        if let Some(window) = app.get_webview_window(&label) {
            return window.show().map_err(CommandError::internal);
        }
        let window =
            tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::App(url.into()))
                .title("Cardbe")
                .inner_size(width, height)
                .min_inner_size(160.0, 120.0)
                .visible(false)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .skip_taskbar(true)
                .maximizable(false)
                .minimizable(false)
                .build()
                .map_err(CommandError::internal)?;
        let (send, receive) = tokio::sync::oneshot::channel();
        let target = window.clone();
        let scheduled = window
            .run_on_main_thread(move || {
                let result = target
                    .hwnd()
                    .map_err(CommandError::internal)
                    .and_then(|hwnd| {
                        native::set_mode(hwnd.0, true).map_err(CommandError::internal)
                    });
                if result.is_ok() {
                    shape(target.hwnd().map(|hwnd| hwnd.0), target.scale_factor());
                }
                let _ = send.send(result);
            })
            .map_err(CommandError::internal);
        let attached = match scheduled {
            Ok(()) => receive
                .await
                .map_err(CommandError::internal)
                .and_then(|result| result),
            Err(error) => Err(error),
        };
        if let Err(error) = attached {
            let _ = window.close();
            return Err(error);
        }
        window.show().map_err(CommandError::internal)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, label, url, width, height);
        Err(CommandError::InvalidArgument)
    }
}

/// Logical corner radius shared with the widget card (`rounded-2xl`).
#[cfg(windows)]
const CORNER_RADIUS: f64 = 16.0;

/// Clips a desktop widget to its rounded card. Child windows of the desktop
/// are not composited with per-pixel transparency, so transparent margins
/// would show as a rectangle. Runs on the UI thread after attaching and resizing.
#[cfg(windows)]
pub fn shape(hwnd: tauri::Result<windows_sys::Win32::Foundation::HWND>, scale: tauri::Result<f64>) {
    let result = hwnd.map_err(|error| error.to_string()).and_then(|hwnd| {
        let radius = (CORNER_RADIUS * scale.unwrap_or(1.0)).round() as i32;
        native::round_corners(hwnd, radius).map_err(|error| error.to_string())
    });
    if let Err(error) = result {
        log::warn!(target: "desktop", "Could not shape a desktop widget: {error}");
    }
}

#[cfg(windows)]
mod native {
    use std::{ffi::c_void, io, ptr};
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError, HWND, LPARAM, POINT, RECT};

    // Only user32 functions are needed; keep the existing dependency/lockfile intact.
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            callback: unsafe extern "system" fn(HWND, LPARAM) -> i32,
            data: LPARAM,
        ) -> i32;
        fn FindWindowExW(parent: HWND, after: HWND, class: *const u16, title: *const u16) -> HWND;
        fn GetClassNameW(window: HWND, class: *mut u16, count: i32) -> i32;
        fn GetParent(window: HWND) -> HWND;
        fn SetParent(window: HWND, parent: HWND) -> HWND;
        fn GetWindowLongW(window: HWND, index: i32) -> i32;
        fn SetWindowLongW(window: HWND, index: i32, value: i32) -> i32;
        fn GetWindowRect(window: HWND, rect: *mut RECT) -> i32;
        fn MapWindowPoints(from: HWND, to: HWND, points: *mut POINT, count: u32) -> i32;
        fn SetWindowPos(
            window: HWND,
            after: HWND,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            flags: u32,
        ) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowRgn(window: HWND, region: *mut c_void, redraw: i32) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> *mut c_void;
        fn DeleteObject(object: *mut c_void) -> i32;
    }

    pub fn round_corners(window: HWND, radius: i32) -> io::Result<()> {
        // SAFETY: Called on the UI thread with a live Cardbe window. The system
        // owns the region once SetWindowRgn succeeds; otherwise it is deleted here.
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(window, &mut rect) == 0 {
                return Err(io::Error::last_os_error());
            }
            // The right and bottom coordinates are exclusive.
            let region = CreateRoundRectRgn(
                0,
                0,
                rect.right - rect.left + 1,
                rect.bottom - rect.top + 1,
                radius * 2,
                radius * 2,
            );
            if region.is_null() {
                return Err(io::Error::other("Could not create a window region"));
            }
            if SetWindowRgn(window, region, 1) == 0 {
                DeleteObject(region);
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    const GWL_STYLE: i32 = -16;
    const WS_CHILD: i32 = 0x4000_0000;
    const WS_POPUP: i32 = 0x8000_0000u32 as i32;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;

    fn mode_style(style: i32, enabled: bool) -> i32 {
        if enabled {
            (style & !WS_POPUP) | WS_CHILD
        } else {
            (style & !WS_CHILD) | WS_POPUP
        }
    }

    unsafe extern "system" fn find_desktop(window: HWND, data: LPARAM) -> i32 {
        let class: Vec<u16> = "SHELLDLL_DefView\0".encode_utf16().collect();
        // SAFETY: EnumWindows supplies a live HWND; data points to our synchronous
        // caller's HWND slot, and the class name is null-terminated.
        unsafe {
            let mut host_class = [0u16; 32];
            let length = GetClassNameW(window, host_class.as_mut_ptr(), host_class.len() as i32);
            let host_class = String::from_utf16_lossy(&host_class[..length as usize]);
            if host_class != "Progman" && host_class != "WorkerW" {
                return 1;
            }
            if !FindWindowExW(window, ptr::null_mut(), class.as_ptr(), ptr::null()).is_null() {
                *(data as *mut HWND) = window;
                return 0;
            }
        }
        1
    }

    pub fn set_mode(window: HWND, enabled: bool) -> io::Result<()> {
        // SAFETY: Called on the UI thread with the invoking live Cardbe window.
        // Shell handles are borrowed and never destroyed or subclassed.
        unsafe {
            let style = GetWindowLongW(window, GWL_STYLE);
            if (style & WS_CHILD != 0) == enabled {
                return Ok(());
            }
            let mut parent: HWND = ptr::null_mut();
            if enabled {
                // shortcut: Explorer's desktop hierarchy is not a stable public API;
                // revalidate on Windows updates before extending supported shells.
                EnumWindows(find_desktop, ptr::from_mut(&mut parent) as LPARAM);
                if parent.is_null() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Desktop host unavailable",
                    ));
                }
            }
            let previous_parent = GetParent(window);
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(window, &mut rect) == 0 {
                return Err(io::Error::last_os_error());
            }
            SetLastError(0);
            if SetWindowLongW(window, GWL_STYLE, mode_style(style, enabled)) == 0
                && GetLastError() != 0
            {
                return Err(io::Error::last_os_error());
            }
            SetLastError(0);
            if SetParent(window, parent).is_null() && GetLastError() != 0 {
                let error = io::Error::last_os_error();
                SetWindowLongW(window, GWL_STYLE, style);
                return Err(error);
            }
            let result = place(window, parent, &rect, enabled);
            if result.is_err() {
                SetParent(window, previous_parent);
                SetWindowLongW(window, GWL_STYLE, style);
                let _ = place(window, previous_parent, &rect, !enabled);
            }
            result
        }
    }

    unsafe fn place(window: HWND, parent: HWND, rect: &RECT, attached: bool) -> io::Result<()> {
        let mut point = POINT {
            x: rect.left,
            y: rect.top,
        };
        // SAFETY: Handles and rectangle come from set_mode; MapWindowPoints
        // converts screen coordinates to the desktop host's client coordinates.
        unsafe {
            if !parent.is_null() {
                SetLastError(0);
                if MapWindowPoints(ptr::null_mut(), parent, &mut point, 1) == 0
                    && GetLastError() != 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
            // A child goes above desktop icons, but remains below other applications.
            // Detached windows resume the existing always-on-top floating behavior.
            let after = if attached {
                ptr::null_mut()
            } else {
                -1isize as HWND
            };
            if SetWindowPos(
                window,
                after,
                point.x,
                point.y,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    #[test]
    fn mode_changes_only_child_and_popup_bits() {
        let original = 0x10ce0000 | WS_POPUP;
        let attached = mode_style(original, true);
        assert_ne!(attached & WS_CHILD, 0);
        assert_eq!(attached & WS_POPUP, 0);
        assert_eq!(mode_style(attached, false), original);
        assert_eq!(mode_style(attached, true), attached);
    }

    #[test]
    #[ignore = "requires an interactive Windows Explorer desktop"]
    fn desktop_attachment_round_trip() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn CreateWindowExW(
                ex: u32,
                class: *const u16,
                title: *const u16,
                style: u32,
                x: i32,
                y: i32,
                w: i32,
                h: i32,
                parent: HWND,
                menu: HWND,
                instance: HWND,
                param: *const std::ffi::c_void,
            ) -> HWND;
            fn DestroyWindow(window: HWND) -> i32;
            fn SetThreadDpiAwarenessContext(context: isize) -> isize;
        }
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                // SAFETY: The test owns this window, including during unwinding.
                unsafe {
                    DestroyWindow(self.0);
                }
            }
        }
        // SAFETY: Built-in STATIC class, no custom procedure, no user data;
        // all calls run on the thread that owns this hidden synthetic window.
        unsafe {
            // Match Tauri's per-monitor DPI-aware UI thread, rather than the
            // DPI-unaware default of a standalone Rust test executable.
            let previous_dpi = SetThreadDpiAwarenessContext(-4);
            assert_ne!(previous_dpi, 0);
            struct DpiContext(isize);
            impl Drop for DpiContext {
                fn drop(&mut self) {
                    // SAFETY: Restores the context returned on this same thread.
                    unsafe {
                        SetThreadDpiAwarenessContext(self.0);
                    }
                }
            }
            let _dpi = DpiContext(previous_dpi);
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let window = TestWindow(CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                WS_POPUP as u32,
                80,
                90,
                320,
                220,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            ));
            assert!(!window.0.is_null(), "{}", io::Error::last_os_error());
            let mut before: RECT = std::mem::zeroed();
            assert_ne!(GetWindowRect(window.0, &mut before), 0);
            set_mode(window.0, true).unwrap();
            assert!(!GetParent(window.0).is_null());
            assert_ne!(GetWindowLongW(window.0, GWL_STYLE) & WS_CHILD, 0);
            let mut attached: RECT = std::mem::zeroed();
            assert_ne!(GetWindowRect(window.0, &mut attached), 0);
            assert_eq!(
                (before.left, before.top, before.right, before.bottom),
                (attached.left, attached.top, attached.right, attached.bottom)
            );
            set_mode(window.0, true).unwrap();
            set_mode(window.0, false).unwrap();
            assert!(GetParent(window.0).is_null());
            assert_eq!(GetWindowLongW(window.0, GWL_STYLE) & WS_CHILD, 0);
            let mut after: RECT = std::mem::zeroed();
            assert_ne!(GetWindowRect(window.0, &mut after), 0);
            assert_eq!(
                (before.left, before.top, before.right, before.bottom),
                (after.left, after.top, after.right, after.bottom)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Placement, WidgetRef};

    #[test]
    fn removal_targets_only_widget_windows() {
        use Placement::{Desktop, Floating};
        let task: WidgetRef = serde_json::from_value(serde_json::json!({
            "kind": "task", "boardId": 7, "taskId": 2
        }))
        .unwrap();
        assert_eq!(task.label(&Desktop), "widget-task-7-2");
        assert_eq!(task.label(&Floating), "float-task-7-2");
        let focus: WidgetRef = serde_json::from_value(serde_json::json!({
            "kind": "focus", "id": "main/🐱"
        }))
        .unwrap();
        assert!(focus.label(&Desktop).starts_with("widget-focus-"));
        assert!(focus.label(&Floating).starts_with("float-focus-"));
        assert!(!focus.label(&Desktop).contains('/'));
        for invalid in [
            serde_json::json!({ "kind": "window", "label": "main" }),
            serde_json::json!({ "kind": "task", "boardId": 7, "taskId": 2, "label": "main" }),
            serde_json::json!({ "kind": "focus", "id": "main", "label": "main" }),
        ] {
            assert!(serde_json::from_value::<WidgetRef>(invalid).is_err());
        }
        assert!(serde_json::from_value::<Placement>(serde_json::json!("main")).is_err());
    }
}
