//! Desktop integrations run outside the render loop. What every system shares lives here; the
//! tray, notifications, shortcuts, single-instance bridge, pickers and keyring are per system.
use anyhow::Result;
use std::process::Command;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
pub mod shell;
pub use shell::foreground_app;
#[cfg(windows)]
mod shell_win;
#[cfg(target_os = "linux")]
mod shell_x11;

pub enum Event {
    Status(String),
    Screenshot {
        owner: String,
        generation: u64,
        result: Result<std::path::PathBuf, String>,
    },
    Folder(Result<std::path::PathBuf, String>),
    Transcript {
        owner: String,
        generation: u64,
        result: Result<String, String>,
    },
    Activate(String),
    Quit,
    Tray(bool),
    Shortcuts(bool),
    Account {
        generation: u64,
        result: Result<crate::account::Session, String>,
    },
    Update(Result<crate::updates::Release, String>),
    /// A piece of an Ask Lantern answer, for the question `generation` and its `turn`.
    Assistant {
        generation: u64,
        turn: u64,
        piece: crate::october_ai::Piece,
    },
    /// The answer finished; `error` is what to show instead when it failed.
    AssistantDone {
        generation: u64,
        turn: u64,
        error: Option<String>,
    },
    /// A piece of a Point & Ask answer, for the conversation `generation` and its `turn` (index).
    PointAsk {
        generation: u64,
        turn: usize,
        piece: crate::october_ai::Piece,
    },
    PointAskDone {
        generation: u64,
        turn: usize,
        error: Option<String>,
    },
    /// `lantern-engine hooks status` (JSON), after reading or changing the agent hooks.
    Hooks(Result<String, String>),
    /// Point & Ask's message for an agent is ready (its screenshot saved), or couldn't be.
    PointAskSent {
        generation: u64,
        agent_id: String,
        text: Result<String, String>,
    },
}
/// The tray's menu, as the app wants it: `(label, activation id)`, `None` for a separator. Labels
/// change with the app (counts), so trays re-read it when its version changes.
static TRAY_MENU: std::sync::RwLock<Vec<Option<(String, String)>>> = std::sync::RwLock::new(Vec::new());
static TRAY_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_tray_menu(items: Vec<Option<(String, String)>>) {
    let mut menu = TRAY_MENU.write().unwrap_or_else(|e| e.into_inner());
    if *menu != items {
        *menu = items;
        TRAY_VERSION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
pub fn tray_menu() -> Vec<Option<(String, String)>> {
    TRAY_MENU.read().unwrap_or_else(|e| e.into_inner()).clone()
}
/// The Linux tray refreshes its menu when this changes (the Windows tray builds it on each open).
#[cfg(target_os = "linux")]
pub fn tray_menu_version() -> u64 {
    TRAY_VERSION.load(std::sync::atomic::Ordering::SeqCst)
}
/// What choosing a tray item means.
pub fn tray_event(id: &str) -> Event {
    match id {
        "quit" => Event::Quit,
        other => Event::Activate(other.into()),
    }
}

// Wake/map the native window before waiting for another frame. A hidden Wayland window may
// receive no frame callbacks; a repaint request alone cannot reliably activate it from the tray.
pub fn event_pump(wake: impl Fn(bool) + Send + 'static) -> (Sender<Event>, mpsc::Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let (forward, events) = mpsc::channel();
    std::thread::spawn(move || {
        let mut tray = false;
        for event in rx {
            let show = matches!(event, Event::Activate(_)) || matches!(event, Event::Tray(false)) && tray;
            if let Event::Tray(available) = &event {
                tray = *available;
            }
            if forward.send(event).is_err() {
                break;
            }
            wake(show);
        }
    });
    (tx, events)
}
pub fn task<T: Send + 'static>(
    tx: Sender<Event>,
    work: impl FnOnce() -> Result<T> + Send + 'static,
    event: impl FnOnce(Result<T, String>) -> Event + Send + 'static,
) {
    std::thread::spawn(move || {
        let result = work().map_err(|error| format!("{error:#}"));
        let _ = tx.send(event(result));
    });
}
pub fn prune_screenshots() {
    let cutoff = std::time::SystemTime::now() - Duration::from_secs(7 * 86400);
    for entry in std::fs::read_dir(crate::settings::data_dir().join("screenshots")).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "png")
            && entry.metadata().ok().is_some_and(|metadata| metadata.is_file() && metadata.modified().is_ok_and(|time| time < cutoff))
        {
            let _ = std::fs::remove_file(path);
        }
    }
}
/// A command for a helper program, without a console window flashing up on Windows.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(::windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    command
}
pub fn hooks(action: &'static str) -> Result<String> {
    let output = command(crate::sidecar::engine_path()?).args(["hooks", action]).output()?;
    anyhow::ensure!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_activation_wakes_the_window_without_ui_polling() {
        let (wake_tx, wake_rx) = mpsc::channel();
        let (tx, _events) = event_pump(move |show| {
            wake_tx.send(show).unwrap();
        });
        tx.send(Event::Status("background result".into())).unwrap();
        assert!(!wake_rx.recv_timeout(Duration::from_secs(3)).unwrap());
        tx.send(Event::Activate("agent-a".into())).unwrap();
        assert!(wake_rx.recv_timeout(Duration::from_secs(3)).unwrap());
        tx.send(Event::Tray(true)).unwrap();
        assert!(!wake_rx.recv_timeout(Duration::from_secs(3)).unwrap());
        tx.send(Event::Tray(false)).unwrap();
        assert!(wake_rx.recv_timeout(Duration::from_secs(3)).unwrap());
    }
}
