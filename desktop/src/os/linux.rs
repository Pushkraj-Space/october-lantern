//! Linux desktop integration. Portals retain the user's choice of screen and shortcuts on
//! Wayland; Lantern never bypasses a denied portal request.
use super::Event;
use crate::settings::{Settings, data_dir, private_write};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub const BUS_NAME: &str = "dev.october.Lantern";
struct Bridge {
    snapshot: Arc<RwLock<String>>,
    tx: Sender<Event>,
}
#[zbus::interface(name = "dev.october.Lantern")]
impl Bridge {
    fn get_snapshot(&self) -> String {
        self.snapshot.read().map(|value| value.clone()).unwrap_or_else(|_| "[]".into())
    }
    fn show(&self, agent: &str) {
        let _ = self.tx.send(Event::Activate(if agent.is_empty() { "show".into() } else { agent.into() }));
    }
    fn point_ask(&self) {
        let _ = self.tx.send(Event::Activate("capture".into()));
    }
}
pub fn bridge(tx: Sender<Event>, snapshot: Arc<RwLock<String>>) {
    std::thread::spawn(move || {
        let result = (|| -> Result<Connection> {
            Ok(zbus::blocking::connection::Builder::session()?
                .name(BUS_NAME)?
                .serve_at("/dev/october/Lantern", Bridge { tx: tx.clone(), snapshot })?
                .build()?)
        })();
        match result {
            Ok(_connection) => loop {
                std::thread::park();
            },
            Err(error) => {
                let _ = tx.send(Event::Status(format!("Desktop integration: {error:#}")));
            }
        }
    });
}
pub fn show_existing(action: &str) -> bool {
    (|| -> Result<()> {
        let connection = Connection::session()?;
        let proxy = Proxy::new(&connection, BUS_NAME, "/dev/october/Lantern", BUS_NAME)?;
        Ok(proxy.call("Show", &(action,))?)
    })()
    .is_ok()
}
/// Window focus isn't passed between apps on Linux; nothing to allow.
pub fn allow_focus(_pid: u32) {}
pub fn open(path: impl AsRef<std::ffi::OsStr>) -> Result<()> {
    let mut child = Command::new("xdg-open").arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
type Options = HashMap<String, OwnedValue>;
fn text(value: &str) -> OwnedValue {
    Value::from(value).try_to_owned().expect("string value")
}
fn token() -> String {
    format!("lantern{}", uuid::Uuid::new_v4().simple())
}
fn portal<'a>(connection: &'a Connection, interface: &str) -> Result<Proxy<'a>> {
    Ok(Proxy::new(connection, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", interface.to_owned())?)
}
fn connection() -> Result<Connection> {
    let connection = Connection::session()?;
    // Host applications must register before portal calls so the compositor knows their app ID.
    let registry = portal(&connection, "org.freedesktop.host.portal.Registry")?;
    let _: Result<(), _> = registry.call("Register", &("dev.october.lantern", Options::new()));
    drop(registry);
    Ok(connection)
}
fn request(connection: &Connection, token: &str, call: impl FnOnce() -> Result<OwnedObjectPath>) -> Result<Options> {
    let sender = connection.unique_name().context("no session bus name")?.as_str().trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let proxy = Proxy::new(connection, "org.freedesktop.portal.Desktop", path.clone(), "org.freedesktop.portal.Request")?;
    // Subscribe before making the call: a fast portal can finish before the method returns.
    let mut signals = proxy.receive_signal("Response")?;
    let returned = call()?;
    anyhow::ensure!(returned.as_str() == path, "portal returned an unexpected request handle");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = signals.next().context("portal disconnected").and_then(|message| {
            let (code, options): (u32, Options) = message.body().deserialize()?;
            // 1: the person cancelled; 2: denied or failed (e.g. no permission for a non-interactive capture).
            anyhow::ensure!(code == 0, "Desktop request cancelled or denied (code {code})");
            Ok(options)
        });
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(result) => result,
        Err(_) => {
            let _: Result<(), _> = proxy.call("Close", &());
            // Wake the response reader even if a broken portal never emits cancellation.
            let _ = connection.clone().close();
            bail!("Desktop request timed out");
        }
    }
}
/// The whole screen, without the desktop's chooser (the portal asks once to allow it), for New
/// Session's "Include a screenshot of my screen" and Point & Ask's frozen frame.
pub fn capture_screen() -> Result<PathBuf> {
    // GNOME 50 denies a non-interactive capture to a host app registered under an app ID
    // (response 2, no dialog), but asks the person once for an unregistered one. So captures
    // without the chooser go out unregistered; everything else registers.
    let connection = Connection::session()?;
    let proxy = portal(&connection, "org.freedesktop.portal.Screenshot")?;
    let id = token();
    let options = Options::from([("handle_token".into(), text(&id)), ("interactive".into(), false.into())]);
    let mut result = request(&connection, &id, || Ok(proxy.call("Screenshot", &("", options))?))?;
    let uri = String::try_from(result.remove("uri").context("screenshot URI missing")?)?;
    let source = url::Url::parse(&uri)?.to_file_path().map_err(|_| anyhow::anyhow!("screenshot is not a local file"))?;
    let bytes = std::fs::read(&source)?;
    anyhow::ensure!(bytes.len() <= 20 * 1024 * 1024 && bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "screenshot must be a PNG under 20 MB");
    let dest = data_dir().join("screenshots").join(format!("{}.png", uuid::Uuid::new_v4().simple()));
    private_write(&dest, &bytes)?;
    Ok(dest)
}
pub fn diagnostics() -> Result<serde_json::Value> {
    let connection = connection()?;
    let screenshot = portal(&connection, "org.freedesktop.portal.Screenshot")?;
    let shortcuts = portal(&connection, "org.freedesktop.portal.GlobalShortcuts")?;
    let chooser = portal(&connection, "org.freedesktop.portal.FileChooser")?;
    let screenshot_version: u32 = screenshot.get_property("version")?;
    let shortcuts_version: u32 = shortcuts.get_property("version")?;
    let chooser_version: u32 = chooser.get_property("version")?;
    let id = token();
    let session_id = token();
    let options = Options::from([("handle_token".into(), text(&id)), ("session_handle_token".into(), text(&session_id))]);
    let mut response = request(&connection, &id, || Ok(shortcuts.call("CreateSession", &(options,))?))?;
    let handle = String::try_from(response.remove("session_handle").context("session missing")?)?;
    let session = Proxy::new(&connection, "org.freedesktop.portal.Desktop", handle, "org.freedesktop.portal.Session")?;
    let _: () = session.call("Close", &())?;
    Ok(
        serde_json::json!({"screenshot":screenshot_version,"globalShortcuts":shortcuts_version,"fileChooser":chooser_version,"shortcutSession":"created and closed"}),
    )
}
pub fn folder() -> Result<PathBuf> {
    let connection = connection()?;
    let proxy = portal(&connection, "org.freedesktop.portal.FileChooser")?;
    let id = token();
    let options = Options::from([("handle_token".into(), text(&id)), ("directory".into(), true.into()), ("multiple".into(), false.into())]);
    let mut result = request(&connection, &id, || Ok(proxy.call("OpenFile", &("", "Choose a project folder", options))?))?;
    let uris = Vec::<String>::try_from(result.remove("uris").context("folder URI missing")?)?;
    url::Url::parse(uris.first().context("no folder selected")?)?.to_file_path().map_err(|_| anyhow::anyhow!("Choose a local folder"))
}
pub fn shortcuts(tx: Sender<Event>) {
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            let connection = connection()?;
            let proxy = portal(&connection, "org.freedesktop.portal.GlobalShortcuts")?;
            let id = token();
            let session_id = token();
            let options = Options::from([("handle_token".into(), text(&id)), ("session_handle_token".into(), text(&session_id))]);
            let mut result = request(&connection, &id, || Ok(proxy.call("CreateSession", &(options,))?))?;
            let session =
                OwnedObjectPath::try_from(String::try_from(result.remove("session_handle").context("missing shortcut session")?)?)?;
            let id = token();
            let bindings = vec![
                (
                    "compose",
                    Options::from([
                        ("description".into(), text("Lantern: Message an agent")),
                        ("preferred_trigger".into(), text("CTRL+ALT+space")),
                    ]),
                ),
                (
                    "capture",
                    Options::from([("description".into(), text("Lantern: Point & Ask")), ("preferred_trigger".into(), text("CTRL+ALT+p"))]),
                ),
            ];
            let mut activated = proxy.receive_signal("Activated")?;
            request(&connection, &id, || {
                Ok(proxy.call("BindShortcuts", &(&session, bindings, "", Options::from([("handle_token".into(), text(&id))])))?)
            })?;
            let _ = tx.send(Event::Shortcuts(true));
            for message in &mut activated {
                let (handle, shortcut, _, _): (OwnedObjectPath, String, u64, Options) = message.body().deserialize()?;
                if handle == session && tx.send(Event::Activate(shortcut)).is_err() {
                    break;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = tx.send(Event::Shortcuts(false));
            let _ = tx.send(Event::Status(format!("Shortcuts: {error:#}")));
        }
    });
}
pub fn notify(tx: Sender<Event>, agent: String, title: String, body: String) {
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            let connection = Connection::session()?;
            let proxy = Proxy::new(
                &connection,
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
            )?;
            let mut signals = proxy.receive_all_signals()?;
            let hints = Options::from([("desktop-entry".into(), text("dev.october.lantern"))]);
            let id: u32 = proxy.call(
                "Notify",
                &("October Lantern", 0u32, "dev.october.lantern", title, body, vec!["default", "Open session"], hints, 10000i32),
            )?;
            for signal in &mut signals {
                match signal.header().member().map(|member| member.as_str()) {
                    Some("ActionInvoked") => {
                        let (notification, _): (u32, String) = signal.body().deserialize()?;
                        if notification == id {
                            let _ = tx.send(Event::Activate(agent));
                            break;
                        }
                    }
                    Some("NotificationClosed") => {
                        let (notification, _): (u32, u32) = signal.body().deserialize()?;
                        if notification == id {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = tx.send(Event::Status(format!("Notification: {error:#}")));
        }
    });
}
pub fn remove_integrations() -> Result<String> {
    super::hooks("uninstall")?;
    for path in [
        crate::settings::autostart_path(),
        crate::settings::xdg("XDG_DATA_HOME", ".local/share").join("applications/dev.october.lantern.desktop"),
        crate::settings::xdg("XDG_DATA_HOME", ".local/share").join("icons/hicolor/256x256/apps/dev.october.lantern.png"),
    ] {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    let _ = Command::new("gnome-extensions").args(["disable", "lantern@october.dev"]).output();
    let extension = crate::settings::xdg("XDG_DATA_HOME", ".local/share").join("gnome-shell/extensions/lantern@october.dev");
    if extension.exists() {
        std::fs::remove_dir_all(extension)?;
    }
    Ok("Desktop integrations removed. Your projects, agent sessions, settings, and downloaded speech model are kept.".into())
}
struct LanternTray {
    tx: Sender<Event>,
}
impl ksni::Tray for LanternTray {
    fn id(&self) -> String {
        "dev.october.lantern".into()
    }
    fn title(&self) -> String {
        "October Lantern".into()
    }
    fn icon_name(&self) -> String {
        "dev.october.lantern".into()
    }
    fn activate(&mut self, _: i32, _: i32) {
        let _ = self.tx.send(Event::Activate("show".into()));
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        super::tray_menu()
            .into_iter()
            .map(|item| match item {
                None => ksni::MenuItem::Separator,
                Some((label, id)) => StandardItem {
                    label,
                    activate: Box::new(move |tray: &mut Self| {
                        let _ = tray.tx.send(super::tray_event(&id));
                    }),
                    ..Default::default()
                }
                .into(),
            })
            .collect()
    }
}
fn tray_host() -> bool {
    (|| -> Result<bool> {
        let connection = Connection::session()?;
        let proxy = Proxy::new(&connection, "org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher")?;
        Ok(proxy.get_property("IsStatusNotifierHostRegistered")?)
    })()
    .unwrap_or(false)
}
pub fn tray(tx: Sender<Event>) {
    std::thread::spawn(move || {
        use ksni::blocking::TrayMethods;
        let mut handle: Option<ksni::blocking::Handle<LanternTray>> = None;
        let mut last = None;
        let mut shown = 0;
        loop {
            if handle.as_ref().is_some_and(|handle| handle.is_closed()) {
                handle = None;
            }
            let host = tray_host();
            if host && handle.is_none() {
                handle = (LanternTray { tx: tx.clone() }).spawn().ok();
            }
            let available = host && handle.is_some();
            if last != Some(available) {
                if tx.send(Event::Tray(available)).is_err() {
                    break;
                }
                last = Some(available);
            }
            // Refresh the menu when the app changes it (counts); look for a tray host every 10 s.
            for _ in 0..10 {
                if let Some(handle) = &handle
                    && shown != super::tray_menu_version()
                {
                    shown = super::tray_menu_version();
                    handle.update(|_| {});
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}
pub struct Recording {
    child: Child,
    path: PathBuf,
}
impl Recording {
    pub fn start() -> Result<Self> {
        let path = data_dir().join("dictation").join(format!("{}.wav", uuid::Uuid::new_v4().simple()));
        private_write(&path, b"")?;
        let child = Command::new("pw-record")
            .args(["--rate", "16000", "--channels", "1", "--format", "s16"])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("PipeWire recorder is unavailable")?;
        Ok(Self { child, path })
    }
    pub fn finish(mut self, settings: Settings) -> Result<String> {
        // SIGINT lets pw-record finish the WAV header; kill() would leave an invalid recording.
        let _ = Command::new("kill").args(["-INT", &self.child.id().to_string()]).status();
        self.child.wait()?;
        let output = Command::new(&settings.whisper)
            .args(["-m", &settings.whisper_model, "-f"])
            .arg(&self.path)
            .args(["-l", &settings.speech_language, "-nt", "-np"])
            .output()
            .context("Run scripts/setup-dictation.sh or configure a whisper.cpp executable and model in Settings")?;
        anyhow::ensure!(output.status.success(), "Transcription failed: {}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout)?.trim().to_owned();
        anyhow::ensure!(!text.is_empty(), "No speech detected");
        Ok(text)
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.path);
    }
}
/// Whether the process is still running (for waiting on the instance being replaced).
pub fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}
const SECRET: [&str; 4] = ["application", "dev.october.lantern", "credential", "refresh-token"];
/// Saves the October refresh token in the Secret Service keyring.
pub fn secret_store(token: &str) -> Result<()> {
    let mut child = Command::new("secret-tool")
        .args(["store", "--label=October Lantern"])
        .args(SECRET)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child.stdin.take().context("Secret Service input unavailable")?.write_all(token.as_bytes())?;
    anyhow::ensure!(child.wait()?.success(), "Secret Service could not save sign-in");
    Ok(())
}
pub fn secret_lookup() -> Result<String> {
    let output = Command::new("secret-tool").arg("lookup").args(SECRET).output()?;
    anyhow::ensure!(output.status.success(), "No saved October sign-in");
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
pub fn secret_clear() {
    let _ = Command::new("secret-tool").arg("clear").args(SECRET).output();
}
