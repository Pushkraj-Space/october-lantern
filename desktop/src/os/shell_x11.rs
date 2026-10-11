//! The pill and panel on X11/XWayland (CONTRACT §7, proven in the Phase 0 spike on GNOME 50):
//! work areas from `_NET_WORKAREA` and RandR, and, once a window is mapped, EWMH state (above,
//! every workspace, out of the taskbar and pager) plus a no-input hint for the pill.
use super::shell::Role;
use eframe::egui::{Rect, pos2, vec2};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, PropMode, Window};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// Lantern's windows run on X11 (XWayland under Wayland) when an X server is there: Wayland gives
/// clients no position, stacking or workspace control, which the edge pill needs.
pub fn x11_available() -> bool {
    std::env::var_os("DISPLAY").is_some()
}

struct X {
    conn: RustConnection,
    root: Window,
    atoms: HashMap<&'static str, u32>,
}

impl X {
    fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots[screen].root;
        Some(Self { conn, root, atoms: HashMap::new() })
    }
    fn atom(&mut self, name: &'static str) -> u32 {
        if let Some(atom) = self.atoms.get(name) {
            return *atom;
        }
        let atom = self.conn.intern_atom(false, name.as_bytes()).ok().and_then(|c| c.reply().ok()).map_or(0, |r| r.atom);
        self.atoms.insert(name, atom);
        atom
    }
    fn cardinals(&mut self, window: Window, name: &'static str, kind: impl Into<u32>) -> Vec<u32> {
        let atom = self.atom(name);
        self.conn
            .get_property(false, window, atom, kind, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
    }
    fn title(&mut self, window: Window) -> String {
        let (name, utf8) = (self.atom("_NET_WM_NAME"), self.atom("UTF8_STRING"));
        self.conn
            .get_property(false, window, name, utf8, 0, 256)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
            .unwrap_or_default()
    }
    /// One of this process's managed windows, by title.
    fn find(&mut self, title: &str) -> Option<Window> {
        let pid = std::process::id();
        let clients = self.cardinals(self.root, "_NET_CLIENT_LIST", AtomEnum::WINDOW);
        clients
            .into_iter()
            .find(|&window| self.cardinals(window, "_NET_WM_PID", AtomEnum::CARDINAL).first() == Some(&pid) && self.title(window) == title)
    }
    /// Asks the window manager to change a mapped window's state (EWMH client message).
    fn request(&mut self, window: Window, message: &'static str, data: [u32; 5]) {
        let kind = self.atom(message);
        let event = ClientMessageEvent::new(32, window, kind, data);
        let _ = self.conn.send_event(false, self.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event);
    }
}

/// Work areas in physical pixels, one per monitor: each RandR monitor clipped to `_NET_WORKAREA`
/// (which excludes panels like GNOME's top bar). Empty when there is no X server.
pub fn work_areas() -> Vec<Rect> {
    let Some(mut x) = X::connect() else { return Vec::new() };
    let root = x.root;
    let work = x.cardinals(root, "_NET_WORKAREA", AtomEnum::CARDINAL);
    let screen = &x.conn.setup().roots[0];
    let whole = Rect::from_min_size(pos2(0.0, 0.0), vec2(screen.width_in_pixels as f32, screen.height_in_pixels as f32));
    let work = if work.len() >= 4 {
        Rect::from_min_size(pos2(work[0] as f32, work[1] as f32), vec2(work[2] as f32, work[3] as f32))
    } else {
        whole
    };
    let monitors: Vec<Rect> = x
        .conn
        .randr_get_monitors(root, true)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|reply| {
            reply
                .monitors
                .iter()
                .map(|m| Rect::from_min_size(pos2(m.x as f32, m.y as f32), vec2(m.width as f32, m.height as f32)))
                .collect()
        })
        .unwrap_or_default();
    if monitors.is_empty() {
        return vec![work];
    }
    monitors.into_iter().map(|monitor| monitor.intersect(work)).filter(|area| area.is_positive()).collect()
}

/// Gives a mapped Lantern window its role (CONTRACT §7). Returns false until the window exists,
/// so the caller tries again next frame.
pub fn adopt(title: &str, role: Role) -> bool {
    let Some(mut x) = X::connect() else { return false };
    let Some(window) = x.find(title) else { return false };
    if role == Role::Pill {
        // WM_HINTS: flags = InputHint, input = false. Clicks still arrive; focus stays where it was.
        let hints = AtomEnum::WM_HINTS;
        let _ = x.conn.change_property32(PropMode::REPLACE, window, hints, hints, &[1, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
    let (above, sticky) = (x.atom("_NET_WM_STATE_ABOVE"), x.atom("_NET_WM_STATE_STICKY"));
    let (taskbar, pager) = (x.atom("_NET_WM_STATE_SKIP_TASKBAR"), x.atom("_NET_WM_STATE_SKIP_PAGER"));
    // _NET_WM_STATE_ADD = 1; source indication 1 = a normal application.
    x.request(window, "_NET_WM_STATE", [1, above, sticky, 1, 0]);
    x.request(window, "_NET_WM_STATE", [1, taskbar, pager, 1, 0]);
    x.request(window, "_NET_WM_DESKTOP", [0xFFFF_FFFF, 1, 0, 0, 0]);
    x.conn.flush().is_ok()
}

/// The global pointer isn't reliable under XWayland (stale over Wayland-native windows), so hover
/// comes from Lantern's own windows (CONTRACT §8).
pub fn cursor_position() -> Option<eframe::egui::Pos2> {
    None
}
