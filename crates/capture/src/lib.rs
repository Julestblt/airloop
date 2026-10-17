//! Audio capture for Airloop.
//!
//! On Windows this uses WASAPI:
//! - **System audio**: loopback capture of the default render device. Follows
//!   the default device when the user switches outputs.
//! - **One application**: process loopback (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`,
//!   Windows 10 build 20348 / Windows 11 and later), capturing the process and
//!   its children before it reaches the mixer.
//!
//! Both deliver 44.1 kHz stereo `f32`, which is what the engine streams. On
//! other platforms every entry point returns [`SourceError::Unsupported`].

use airloop_engine::{AudioSource, SourceError};
use serde::Serialize;

#[cfg(windows)]
mod windows_impl;

/// What to capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    /// Everything this PC plays through its default output.
    System,
    /// One process and its child processes.
    Process { pid: u32 },
}

/// An application that currently has an audio session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioApp {
    /// Process id to capture (the top-level process of the app).
    pub pid: u32,
    /// Display name, e.g. "Spotify".
    pub name: String,
    /// Executable file name, e.g. `Spotify.exe`.
    pub exe: String,
    /// Whether the app is producing sound right now.
    pub active: bool,
}

/// Opens a capture source. Capture starts when the engine calls `start`.
pub fn open_source(target: CaptureTarget) -> Result<Box<dyn AudioSource>, SourceError> {
    #[cfg(windows)]
    {
        Ok(Box::new(windows_impl::WasapiSource::new(target)))
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err(SourceError::Unsupported)
    }
}

/// Lists applications with an audio session on any output device, most
/// recently active first.
pub fn list_audio_apps() -> Result<Vec<AudioApp>, SourceError> {
    #[cfg(windows)]
    {
        windows_impl::list_audio_apps()
    }
    #[cfg(not(windows))]
    {
        Err(SourceError::Unsupported)
    }
}

/// Whether per-application capture is available on this OS version.
pub fn supports_app_capture() -> bool {
    #[cfg(windows)]
    {
        windows_impl::supports_process_loopback()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Turns `spotify.exe` into `Spotify`, `msedge.exe` into `Microsoft Edge`.
pub fn pretty_app_name(exe: &str) -> String {
    let stem = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    let stem = stem
        .strip_suffix(".exe")
        .or_else(|| stem.strip_suffix(".EXE"))
        .unwrap_or(stem);
    let known = match stem.to_ascii_lowercase().as_str() {
        "chrome" => Some("Google Chrome"),
        "msedge" => Some("Microsoft Edge"),
        "firefox" => Some("Firefox"),
        "brave" => Some("Brave"),
        "opera" => Some("Opera"),
        "spotify" => Some("Spotify"),
        "vlc" => Some("VLC"),
        "discord" => Some("Discord"),
        "teams" | "ms-teams" => Some("Microsoft Teams"),
        "applemusic" => Some("Apple Music"),
        "itunes" => Some("iTunes"),
        "deezer" => Some("Deezer"),
        "tidal" => Some("TIDAL"),
        "foobar2000" => Some("foobar2000"),
        "wmplayer" => Some("Windows Media Player"),
        "zune" | "microsoft.media.player" => Some("Media Player"),
        _ => None,
    };
    match known {
        Some(k) => k.to_string(),
        None => {
            let mut c = stem.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => exe.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_app_name("chrome.exe"), "Google Chrome");
        assert_eq!(
            pretty_app_name(r"C:\Program Files\VideoLAN\VLC\vlc.exe"),
            "VLC"
        );
        assert_eq!(pretty_app_name("someplayer.exe"), "Someplayer");
        assert_eq!(pretty_app_name("Spotify.exe"), "Spotify");
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_off_windows() {
        assert!(matches!(
            open_source(CaptureTarget::System),
            Err(SourceError::Unsupported)
        ));
        assert!(!supports_app_capture());
    }
}
