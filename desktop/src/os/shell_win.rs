//! The pill and panel on Windows (CONTRACT §7). UNVERIFIED on a real machine (§8.2c): topmost and
//! tool-window styles keep them on top and out of the taskbar and Alt-Tab; the pill also never
//! activates. Presence on every virtual desktop is not established by these styles alone.
use super::shell::Role;
use eframe::egui::{Pos2, Rect, pos2};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId, HWND_TOPMOST, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST,
};

/// Work areas (each monitor without its taskbar), in physical pixels.
pub fn work_areas() -> Vec<Rect> {
    unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        let areas = unsafe { &mut *(data as *mut Vec<Rect>) };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
            let r = info.rcWork;
            areas.push(Rect::from_min_max(pos2(r.left as f32, r.top as f32), pos2(r.right as f32, r.bottom as f32)));
        }
        1
    }
    let mut areas: Vec<Rect> = Vec::new();
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut areas as *mut _ as LPARAM) };
    areas
}

/// This process's top-level window with `title`.
fn find(title: &str) -> Option<HWND> {
    struct Search {
        title: Vec<u16>,
        pid: u32,
        found: HWND,
    }
    unsafe extern "system" fn each(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if pid != search.pid {
            return 1;
        }
        let mut text = [0u16; 256];
        let len = unsafe { GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32) } as usize;
        if text[..len] == search.title[..] {
            search.found = window;
            return 0;
        }
        1
    }
    let mut search = Search { title: title.encode_utf16().collect(), pid: std::process::id(), found: std::ptr::null_mut() };
    unsafe { EnumWindows(Some(each), &mut search as *mut _ as LPARAM) };
    (!search.found.is_null()).then_some(search.found)
}

/// Gives a Lantern window its role. Returns false until the window exists.
pub fn adopt(title: &str, role: Role) -> bool {
    let Some(window) = find(title) else { return false };
    unsafe {
        let mut style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        style = (style & !WS_EX_APPWINDOW) | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        if role == Role::Pill {
            style |= WS_EX_NOACTIVATE;
        }
        SetWindowLongPtrW(window, GWL_EXSTYLE, style as isize);
        SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
    true
}

/// The global pointer, in physical pixels: reliable on Windows, so the pill keeps the Mac's
/// polled 6pt hover halo.
pub fn cursor_position() -> Option<Pos2> {
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then(|| pos2(point.x as f32, point.y as f32))
}
