use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Result as TauriResult, Size, Position, WebviewWindow};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum WindowState {
    Collapsed,
    Expanded,
    ExpandedWide,
    Notification,
}

pub fn apply_window_state(app: &AppHandle, state: WindowState) -> TauriResult<()> {
    let window = app
        .get_webview_window("live")
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let (width, height) = match state {
        WindowState::Collapsed => (180.0, 36.0),
        WindowState::Expanded => (560.0, 340.0),
        WindowState::ExpandedWide => (680.0, 420.0),
        WindowState::Notification => (360.0, 52.0),
    };
    let monitor = window.current_monitor()?.or(window.primary_monitor()?);
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let size = monitor.size();
        let origin = monitor.position();
        let width_px = (width * scale) as i32;
        let height_px = (height * scale) as i32;
        let x = origin.x + (size.width as i32 - width_px) / 2;
        let y = origin.y + (24.0 * scale) as i32;
        window.set_size(Size::Physical(tauri::PhysicalSize { width: width_px as u32, height: height_px as u32 }))?;
        window.set_position(Position::Physical(tauri::PhysicalPosition { x, y }))?;
    } else {
        window.set_size(Size::Logical(tauri::LogicalSize { width, height }))?;
    }
    Ok(())
}

pub fn apply_stealth(w: &WebviewWindow, on: bool) -> TauriResult<()> {
    w.set_content_protected(on)?;

    #[cfg(target_os = "windows")]
    {
        let hwnd = w.hwnd();
        let hwnd = match hwnd {
            Ok(hwnd) => hwnd,
            Err(_) => return Ok(()),
        };

        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
        };

        let affinity = if on {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            windows::Win32::UI::WindowsAndMessaging::WINDOW_DISPLAY_AFFINITY(0)
        };

        // The native Windows call is only a fallback. If it fails, we keep the app alive and
        // rely on the Tauri window API rather than crashing the process.
        let _ = unsafe { SetWindowDisplayAffinity(HWND(hwnd.0 as _), affinity) };
    }

    Ok(())
}

pub fn set_click_through(w: &WebviewWindow, on: bool) -> TauriResult<()> {
    w.set_ignore_cursor_events(on)
}

pub fn set_topmost(w: &WebviewWindow, on: bool) -> TauriResult<()> {
    w.set_always_on_top(on)
}
