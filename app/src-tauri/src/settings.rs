//! User preferences, stored as JSON in the app config directory.
//!
//! Nothing secret lives here: HomePods use transient pairing, which leaves no
//! credentials behind. If persistent (PIN) pairing is added, its keys belong
//! in the Windows Credential Manager, not in this file.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub last_speaker_ids: Vec<String>,
    /// "system" or "app".
    pub source_kind: String,
    pub last_app_exe: Option<String>,
    pub volume: u8,
    pub auto_reconnect: bool,
    /// Unused since the island UI (0.2); kept so older settings files load.
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_speaker_ids: Vec::new(),
            source_kind: "system".into(),
            last_app_exe: None,
            volume: airloop_engine::volume::DEFAULT_PERCENT,
            auto_reconnect: true,
            close_to_tray: true,
        }
    }
}

impl Settings {
    fn sanitized(mut self) -> Self {
        self.volume = self.volume.min(100);
        if self.source_kind != "app" {
            self.source_kind = "system".into();
        }
        self
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Loads settings, falling back to defaults (with a warning) if the file
    /// is missing or unreadable.
    pub fn load(path: PathBuf) -> Self {
        let current = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<Settings>(&text).unwrap_or_else(|e| {
                warn!(error = %e, "settings file is invalid, using defaults");
                Settings::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                warn!(error = %e, "cannot read settings, using defaults");
                Settings::default()
            }
        };
        Self {
            path,
            current: Mutex::new(current.sanitized()),
        }
    }

    pub fn get(&self) -> Settings {
        self.current
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Updates and persists atomically (write to a temp file, then rename).
    pub fn set(&self, settings: Settings) -> std::io::Result<()> {
        let settings = settings.sanitized();
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&settings).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        *self.current.lock().unwrap_or_else(|p| p.into_inner()) = settings;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("airloop-test-{}-{name}.json", std::process::id()))
    }

    #[test]
    fn missing_file_gives_quiet_defaults() {
        let s = SettingsStore::load(temp_path("missing"));
        assert_eq!(s.get(), Settings::default());
        assert!(s.get().volume <= 30);
    }

    #[test]
    fn round_trip_and_sanitize() {
        let path = temp_path("rt");
        let store = SettingsStore::load(path.clone());
        let s = Settings {
            volume: 250,
            source_kind: "bogus".into(),
            last_speaker_ids: vec!["pair-x".into()],
            ..Settings::default()
        };
        store.set(s).expect("save");
        let loaded = SettingsStore::load(path.clone()).get();
        assert_eq!(loaded.volume, 100);
        assert_eq!(loaded.source_kind, "system");
        assert_eq!(loaded.last_speaker_ids, ["pair-x"]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn partial_file_uses_defaults_for_missing_fields() {
        let path = temp_path("partial");
        std::fs::write(&path, r#"{"volume": 40}"#).expect("write");
        let loaded = SettingsStore::load(path.clone()).get();
        assert_eq!(loaded.volume, 40);
        assert!(loaded.auto_reconnect);
        let _ = std::fs::remove_file(path);
    }
}
