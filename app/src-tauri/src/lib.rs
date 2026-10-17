//! Airloop desktop shell: Tauri commands, tray icon, settings and logging
//! around the platform-independent `airloop-engine`.

mod settings;

use airloop_capture::{
    list_audio_apps, open_source, supports_app_capture, AudioApp, CaptureTarget,
};
use airloop_engine::{AudioSource, Engine, EngineState, EngineStatus, Speaker};
use serde::{Deserialize, Serialize};
use settings::{Settings, SettingsStore};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tracing::{error, info, warn};

struct AppState {
    engine: Engine,
    settings: SettingsStore,
    log_dir: PathBuf,
    _log_guard: Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum SourceChoice {
    System,
    App { pid: u32, exe: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    platform: String,
    app_capture_supported: bool,
    capture_supported: bool,
    log_dir: String,
}

type CmdResult<T> = Result<T, String>;

fn build_source(choice: &SourceChoice) -> CmdResult<Box<dyn AudioSource>> {
    // Developer aid on platforms without capture: stream a test tone.
    if !cfg!(windows) && std::env::var("AIRLOOP_TEST_TONE").is_ok() {
        return Ok(Box::new(airloop_engine::ToneSource::new(
            300.0, 0.125, None,
        )));
    }
    let target = match choice {
        SourceChoice::System => CaptureTarget::System,
        SourceChoice::App { pid, exe } => {
            // The pid the UI saw may be stale (app restarted); prefer the
            // current process for the same executable.
            let current = list_audio_apps()
                .ok()
                .and_then(|apps| apps.into_iter().find(|a| a.exe.eq_ignore_ascii_case(exe)))
                .map(|a| a.pid);
            CaptureTarget::Process {
                pid: current.unwrap_or(*pid),
            }
        }
    };
    open_source(target).map_err(|e| match e {
        airloop_engine::SourceError::Unsupported => {
            "Audio capture is only available on Windows.".to_string()
        }
        other => other.to_string(),
    })
}

#[tauri::command]
fn app_info(state: State<'_, AppState>, app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS.to_string(),
        app_capture_supported: supports_app_capture(),
        capture_supported: cfg!(windows),
        log_dir: state.log_dir.display().to_string(),
    }
}

#[tauri::command]
fn get_speakers(state: State<'_, AppState>) -> Vec<Speaker> {
    state.engine.current_speakers()
}

#[tauri::command]
fn get_status(state: State<'_, AppState>) -> EngineStatus {
    state.engine.status().borrow().clone()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    state.settings.set(settings).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_apps() -> CmdResult<Vec<AudioApp>> {
    tauri::async_runtime::spawn_blocking(list_audio_apps)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn connect(
    state: State<'_, AppState>,
    speaker_ids: Vec<String>,
    source: SourceChoice,
) -> CmdResult<()> {
    if speaker_ids.is_empty() {
        return Err("Select at least one speaker.".into());
    }
    let source = build_source(&source)?;
    state.engine.connect(speaker_ids, source).await;
    Ok(())
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.disconnect().await;
    Ok(())
}

#[tauri::command]
async fn set_volume(state: State<'_, AppState>, volume: u8) -> CmdResult<()> {
    state.engine.set_volume(volume).await;
    Ok(())
}

#[tauri::command]
async fn pause(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.pause().await;
    Ok(())
}

#[tauri::command]
async fn resume(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.resume().await;
    Ok(())
}

#[tauri::command]
async fn set_auto_reconnect(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    state.engine.set_auto_reconnect(enabled).await;
    Ok(())
}

#[tauri::command]
fn get_level(state: State<'_, AppState>) -> f32 {
    state.engine.take_level()
}

#[tauri::command]
fn open_log_dir(state: State<'_, AppState>) -> CmdResult<()> {
    tauri_plugin_opener::open_path(&state.log_dir, None::<&str>).map_err(|e| e.to_string())
}

fn init_logging(log_dir: &PathBuf) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        "info,airplay_audio=warn,airplay_client=warn,airplay_timing=warn,airplay_rtsp=warn,mdns_sd=warn".into()
    });
    let file = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("airloop")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)
        .ok();
    let (writer, guard) = match file {
        Some(f) => {
            let (w, g) = tracing_appender::non_blocking(f);
            (Some(w), Some(g))
        }
        None => (None, None),
    };
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));
    let result = match writer {
        Some(w) => registry
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(w),
            )
            .try_init(),
        None => registry.try_init(),
    };
    if result.is_err() {
        eprintln!("logging already initialised");
    }
    guard
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn tray_label(status: &EngineStatus) -> &'static str {
    match status.state {
        EngineState::Streaming
        | EngineState::Paused
        | EngineState::Connecting
        | EngineState::Reconnecting { .. } => "Disconnect",
        EngineState::Idle | EngineState::Error => "Connect to last speakers",
    }
}

fn tooltip(status: &EngineStatus) -> String {
    match &status.state {
        EngineState::Streaming => format!(
            "Airloop — streaming {}",
            status.source.as_deref().unwrap_or("")
        ),
        EngineState::Paused => "Airloop — paused".into(),
        EngineState::Connecting => "Airloop — connecting…".into(),
        EngineState::Reconnecting { .. } => "Airloop — reconnecting…".into(),
        EngineState::Error => "Airloop — error".into(),
        EngineState::Idle => "Airloop".into(),
    }
}

/// Connect with the saved selection, or disconnect, from the tray.
fn tray_toggle(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let status = state.engine.status().borrow().clone();
        if matches!(status.state, EngineState::Idle | EngineState::Error) {
            let s = state.settings.get();
            if s.last_speaker_ids.is_empty() {
                show_main(&app);
                return;
            }
            let choice = match (s.source_kind.as_str(), s.last_app_exe) {
                ("app", Some(exe)) => SourceChoice::App { pid: 0, exe },
                _ => SourceChoice::System,
            };
            match build_source(&choice) {
                Ok(source) => {
                    state.engine.set_volume(s.volume).await;
                    state.engine.connect(s.last_speaker_ids, source).await;
                }
                Err(e) => {
                    warn!(error = %e, "tray connect failed");
                    show_main(&app);
                }
            }
        } else {
            state.engine.disconnect().await;
        }
    });
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Airloop", true, None::<&str>)?;
    let toggle = MenuItem::with_id(
        app,
        "toggle",
        "Connect to last speakers",
        true,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &sep, &quit_item])?;
    let tray = TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or_else(|| tauri::Error::AssetNotFound("icon".into()))?,
        )
        .tooltip("Airloop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "toggle" => tray_toggle(app),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    // Keep the tray label and tooltip in sync with the engine.
    let mut rx = app.state::<AppState>().engine.status();
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let status = rx.borrow_and_update().clone();
            let _ = toggle.set_text(tray_label(&status));
            let _ = tray.set_tooltip(Some(tooltip(&status)));
        }
    });
    Ok(())
}

/// Disconnects cleanly (so the speaker stops at once) and exits.
fn quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let engine = app.state::<AppState>().engine.clone();
        engine.disconnect().await;
        let mut rx = engine.status();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            rx.wait_for(|s| s.state == EngineState::Idle),
        )
        .await;
        app.exit(0);
    });
}

fn forward_events(app: &AppHandle) {
    let engine = app.state::<AppState>().engine.clone();
    let mut speakers = engine.speakers();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while speakers.changed().await.is_ok() {
            let list = speakers.borrow_and_update().clone();
            let _ = handle.emit("speakers", list);
        }
    });
    let mut status = engine.status();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while status.changed().await.is_ok() {
            let s = status.borrow_and_update().clone();
            let _ = handle.emit("status", s);
        }
    });
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let log_dir = handle.path().app_log_dir()?;
            std::fs::create_dir_all(&log_dir)?;
            let guard = init_logging(&log_dir);
            info!(version = %handle.package_info().version, os = std::env::consts::OS, "Airloop starting");

            let config_dir = handle.path().app_config_dir()?;
            let settings = SettingsStore::load(config_dir.join("settings.json"));
            let s = settings.get();
            let engine = tauri::async_runtime::block_on(Engine::start(s.volume, s.auto_reconnect))
                .map_err(|e| {
                    error!(error = %e, "engine failed to start");
                    Box::<dyn std::error::Error>::from(e.to_string())
                })?;
            app.manage(AppState { engine, settings, log_dir, _log_guard: Mutex::new(guard) });
            forward_events(&handle);
            setup_tray(&handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let to_tray = app.state::<AppState>().settings.get().close_to_tray;
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    api.prevent_close();
                    quit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_speakers,
            get_status,
            get_settings,
            save_settings,
            list_apps,
            connect,
            disconnect,
            set_volume,
            pause,
            resume,
            set_auto_reconnect,
            get_level,
            open_log_dir,
        ]);

    if let Err(e) = builder.run(tauri::generate_context!()) {
        error!(error = %e, "Airloop failed to run");
        eprintln!("Airloop failed to run: {e}");
        std::process::exit(1);
    }
}
