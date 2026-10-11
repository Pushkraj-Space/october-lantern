use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `settings.json`: the engine's terminal and local dictation, and the earlier window's choices,
/// which a first run of this app carries over into its preferences (`app::load_state`). Other
/// fields an older file has are ignored.
#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub notifications: bool,
    pub notify_finished: bool,
    pub check_updates: bool,
    pub welcome_seen: bool,
    pub terminal: String,
    pub toolkit: bool,
    pub bus: bool,
    pub whisper: String,
    pub whisper_model: String,
    pub speech_language: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: true,
            notify_finished: false,
            check_updates: false,
            welcome_seen: false,
            terminal: String::new(),
            toolkit: true,
            bus: false,
            whisper: "whisper-cli".into(),
            whisper_model: String::new(),
            speech_language: "auto".into(),
        }
    }
}
pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}
pub fn xdg(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(fallback))
}
/// Shared with the engine (its support folder): screenshots, dictation, hooks' events.
pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        return xdg("LOCALAPPDATA", "AppData/Local").join("October Lantern");
    }
    xdg("XDG_DATA_HOME", ".local/share").join("october-lantern")
}
pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        return xdg("APPDATA", "AppData/Roaming").join("October Lantern");
    }
    xdg("XDG_CONFIG_HOME", ".config").join("october-lantern")
}
/// Writes the whole file or nothing, readable only by the user. (On Windows the folders Lantern
/// writes to are already private to the user.)
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("missing parent folder")?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".lantern-{}", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(temporary);
    result
}
impl Settings {
    pub fn load() -> Result<Self> {
        let path = config_dir().join("settings.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}
#[cfg(target_os = "linux")]
pub fn autostart_path() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("autostart/dev.october.lantern.desktop")
}
pub fn autostart(enable: bool) -> Result<()> {
    autostart_executable(enable, &std::env::current_exe()?)
}
#[cfg(target_os = "linux")]
pub fn autostart_enabled() -> bool {
    autostart_path().exists()
}
#[cfg(windows)]
pub use crate::os::{autostart_enabled, autostart_executable};
#[cfg(target_os = "linux")]
pub fn autostart_executable(enable: bool, executable: &Path) -> Result<()> {
    let path = autostart_path();
    if !enable {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    let executable =
        executable.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"").replace('`', "\\`").replace('$', "\\$").replace('%', "%%");
    private_write(&path, format!("[Desktop Entry]\nType=Application\nName=October Lantern\nExec=\"{executable}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_files_still_load() {
        // The earlier window's own fields (theme, scale, compact…) are ignored; the rest default.
        let settings: Settings = serde_json::from_str("{\"light\":true,\"scale\":1.4,\"terminal\":\"kitty\"}").unwrap();
        assert!(settings.notifications && settings.toolkit && !settings.bus);
        assert_eq!(settings.terminal, "kitty");
    }
    #[test]
    fn atomic_files_are_private_and_replace_complete_content() {
        let path = std::env::temp_dir().join(format!("lantern-settings-{}", uuid::Uuid::new_v4())).join("settings");
        private_write(&path, b"first").unwrap();
        private_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
