//! The engine actor: one task that owns the session, the audio source and the
//! connection state machine. The UI talks to it through [`Engine`].
//!
//! ```text
//!   Idle ──connect──▶ Connecting ──ok──▶ Streaming ◀──resume── Paused
//!     ▲                   │                 │ keep-alive fails     ▲
//!     │                 error               ▼                      │
//!     └──── disconnect ── Error ◀── gives up ── Reconnecting ──────┘ (pause)
//! ```

use crate::discovery::SpeakerDiscovery;
use crate::error::{EngineError, ErrorKind};
use crate::session::{SpeakerStats, StreamSession};
use crate::sink::{FrameSink, SinkClock};
use crate::source::AudioSource;
use crate::speakers::Speaker;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(2);
const MAX_RECONNECT_ATTEMPTS: u32 = 6;
/// Extra tries for a first connection that got no answer. HomePods sometimes
/// ignore requests for a few seconds after a session ended abruptly.
const INITIAL_CONNECT_RETRIES: u32 = 2;
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(3);

/// Connection state shown in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EngineState {
    Idle,
    Connecting,
    Streaming,
    Paused,
    Reconnecting { attempt: u32 },
    Error,
}

/// A user-facing error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiError {
    pub kind: ErrorKind,
    pub message: String,
    /// Technical detail for the diagnostics panel.
    pub detail: String,
}

impl From<&EngineError> for UiError {
    fn from(e: &EngineError) -> Self {
        UiError {
            kind: e.kind(),
            message: e.user_message(),
            detail: e.to_string(),
        }
    }
}

/// Live counters for the diagnostics panel.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub buffer_ms: f32,
    pub drift_ppm: f64,
    pub buffer_underruns: u64,
    pub dropped_chunks: u64,
    pub speakers: Vec<SpeakerStatsView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerStatsView {
    pub speaker_id: String,
    pub packets_sent: u64,
    pub underruns: u64,
}

impl From<SpeakerStats> for SpeakerStatsView {
    fn from(s: SpeakerStats) -> Self {
        Self {
            speaker_id: s.speaker_id,
            packets_sent: s.packets_sent,
            underruns: s.underruns,
        }
    }
}

/// Everything the UI needs to render the connection panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: EngineState,
    /// Speakers selected for the current/last session.
    pub speaker_ids: Vec<String>,
    pub volume: u8,
    pub source: Option<String>,
    pub error: Option<UiError>,
    pub auto_reconnect: bool,
    /// Ask speakers for a shorter buffer (applies to the next connection).
    pub low_latency: bool,
    pub diagnostics: Diagnostics,
}

enum Command {
    Connect {
        speaker_ids: Vec<String>,
        source: Box<dyn AudioSource>,
    },
    Disconnect,
    SetVolume(u8),
    Pause,
    Resume,
    SetAutoReconnect(bool),
    SetLowLatency(bool),
    SessionResult {
        generation: u64,
        result: Result<StreamSession, EngineError>,
    },
}

/// Handle to the engine. Cheap to clone.
#[derive(Clone)]
pub struct Engine {
    tx: mpsc::Sender<Command>,
    status: watch::Receiver<EngineStatus>,
    discovery: Arc<SpeakerDiscovery>,
    sink: FrameSink,
}

impl Engine {
    /// Starts discovery and the engine task. Must run inside Tokio.
    pub async fn start(
        volume: u8,
        auto_reconnect: bool,
        low_latency: bool,
    ) -> Result<Engine, EngineError> {
        let discovery = Arc::new(SpeakerDiscovery::start().await?);
        let (sink, clock) = FrameSink::new();
        let (tx, rx) = mpsc::channel(32);
        let initial = EngineStatus {
            state: EngineState::Idle,
            speaker_ids: Vec::new(),
            volume: volume.min(100),
            source: None,
            error: None,
            auto_reconnect,
            low_latency,
            diagnostics: Diagnostics::default(),
        };
        let (status_tx, status) = watch::channel(initial);
        let actor = Actor {
            rx,
            tx: tx.clone(),
            status: status_tx,
            discovery: Arc::clone(&discovery),
            sink: sink.clone(),
            _clock: clock,
            session: None,
            source: None,
            generation: 0,
            reconnect_at: None,
            attempt: 0,
            initial_retries: 0,
        };
        tokio::spawn(actor.run());
        Ok(Engine {
            tx,
            status,
            discovery,
            sink,
        })
    }

    pub fn speakers(&self) -> watch::Receiver<Vec<Speaker>> {
        self.discovery.subscribe()
    }

    pub fn current_speakers(&self) -> Vec<Speaker> {
        self.discovery.speakers()
    }

    pub fn status(&self) -> watch::Receiver<EngineStatus> {
        self.status.clone()
    }

    /// Peak level since last call, 0.0–1.0.
    pub fn take_level(&self) -> f32 {
        self.sink.take_peak()
    }

    async fn send(&self, c: Command) {
        if self.tx.send(c).await.is_err() {
            error!("engine task has stopped");
        }
    }

    /// Starts streaming `source` to the given speakers, replacing any
    /// current session.
    pub async fn connect(&self, speaker_ids: Vec<String>, source: Box<dyn AudioSource>) {
        self.send(Command::Connect {
            speaker_ids,
            source,
        })
        .await;
    }

    pub async fn disconnect(&self) {
        self.send(Command::Disconnect).await;
    }

    pub async fn set_volume(&self, percent: u8) {
        self.send(Command::SetVolume(percent.min(100))).await;
    }

    pub async fn pause(&self) {
        self.send(Command::Pause).await;
    }

    pub async fn resume(&self) {
        self.send(Command::Resume).await;
    }

    pub async fn set_low_latency(&self, enabled: bool) {
        self.send(Command::SetLowLatency(enabled)).await;
    }

    pub async fn set_auto_reconnect(&self, enabled: bool) {
        self.send(Command::SetAutoReconnect(enabled)).await;
    }
}

struct Actor {
    rx: mpsc::Receiver<Command>,
    tx: mpsc::Sender<Command>,
    status: watch::Sender<EngineStatus>,
    discovery: Arc<SpeakerDiscovery>,
    sink: FrameSink,
    _clock: SinkClock,
    session: Option<StreamSession>,
    source: Option<Box<dyn AudioSource>>,
    /// Bumped on every connect/disconnect; stale session results are dropped.
    generation: u64,
    reconnect_at: Option<tokio::time::Instant>,
    attempt: u32,
    /// Retries used for the current first connection.
    initial_retries: u32,
}

impl Actor {
    fn update(&self, f: impl FnOnce(&mut EngineStatus)) {
        self.status.send_modify(f);
    }

    fn state(&self) -> EngineState {
        self.status.borrow().state.clone()
    }

    async fn run(mut self) {
        let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
        keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let reconnect = async {
                match self.reconnect_at {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                cmd = self.rx.recv() => match cmd {
                    Some(cmd) => self.handle(cmd).await,
                    None => break,
                },
                _ = keepalive.tick() => self.on_keepalive().await,
                () = reconnect => {
                    self.reconnect_at = None;
                    self.begin_session();
                }
            }
        }
        self.teardown(true).await;
    }

    async fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Connect {
                speaker_ids,
                mut source,
            } => {
                self.teardown(true).await;
                self.attempt = 0;
                self.initial_retries = 0;
                let description = source.describe();
                if let Err(e) = source.start(self.sink.clone()) {
                    let e = EngineError::from(e);
                    error!(error = %e, "audio source failed to start");
                    self.update(|s| {
                        s.state = EngineState::Error;
                        s.error = Some(UiError::from(&e));
                    });
                    return;
                }
                info!(source = %description, ?speaker_ids, "connect requested");
                self.source = Some(source);
                self.sink.set_paused(false);
                self.update(|s| {
                    s.speaker_ids = speaker_ids;
                    s.source = Some(description);
                    s.error = None;
                });
                self.begin_session();
            }
            Command::Disconnect => {
                self.teardown(true).await;
                self.update(|s| {
                    s.state = EngineState::Idle;
                    s.error = None;
                    s.source = None;
                    s.diagnostics = Diagnostics::default();
                });
            }
            Command::SetVolume(v) => {
                self.update(|s| s.volume = v);
                if let Some(session) = &mut self.session {
                    session.set_volume(v).await;
                }
            }
            Command::Pause => {
                if self.state() == EngineState::Streaming {
                    self.sink.set_paused(true);
                    if let Some(session) = &mut self.session {
                        session.pause().await;
                    }
                    self.update(|s| s.state = EngineState::Paused);
                }
            }
            Command::Resume => {
                if self.state() == EngineState::Paused {
                    if let Some(session) = &mut self.session {
                        session.resume().await;
                    }
                    self.sink.set_paused(false);
                    self.update(|s| s.state = EngineState::Streaming);
                }
            }
            Command::SetAutoReconnect(enabled) => {
                self.update(|s| s.auto_reconnect = enabled);
            }
            Command::SetLowLatency(enabled) => {
                self.update(|s| s.low_latency = enabled);
            }
            Command::SessionResult { generation, result } => {
                self.on_session_result(generation, result).await;
            }
        }
    }

    /// Resolves the selected speakers and starts connecting in the background.
    fn begin_session(&mut self) {
        let (ids, volume, low_latency) = {
            let s = self.status.borrow();
            (s.speaker_ids.clone(), s.volume, s.low_latency)
        };
        let mut targets = Vec::new();
        for id in &ids {
            match self.discovery.target(id) {
                Some(t) => targets.push(t),
                None => {
                    let e = EngineError::SpeakerNotFound(id.clone());
                    self.fail(&e);
                    return;
                }
            }
        }
        self.generation += 1;
        let generation = self.generation;
        self.sink.begin_generation(generation);
        let reconnecting = self.attempt > 0;
        let attempt = self.attempt;
        self.update(|s| {
            s.state = if reconnecting {
                EngineState::Reconnecting { attempt }
            } else {
                EngineState::Connecting
            };
        });
        let tx = self.tx.clone();
        let sink = self.sink.clone();
        tokio::spawn(async move {
            let result =
                StreamSession::start(targets, volume, &sink, generation, low_latency).await;
            if let Err(Command::SessionResult {
                result: Ok(session),
                ..
            }) = tx
                .send(Command::SessionResult { generation, result })
                .await
                .map_err(|e| e.0)
            {
                sink.install_outputs(generation, Vec::new());
                session.stop().await;
            }
        });
    }

    async fn on_session_result(
        &mut self,
        generation: u64,
        result: Result<StreamSession, EngineError>,
    ) {
        if generation != self.generation {
            // Superseded by a newer connect/disconnect. The stale session
            // installed its outputs; the newer one will replace them.
            if let Ok(session) = result {
                session.stop().await;
            }
            return;
        }
        match result {
            Ok(session) => {
                self.session = Some(session);
                self.attempt = 0;
                self.update(|s| {
                    s.state = EngineState::Streaming;
                    s.error = None;
                });
            }
            Err(e) => {
                warn!(error = %e, attempt = self.attempt, "session start failed");
                self.on_session_lost(&e);
            }
        }
    }

    /// A connect attempt failed: retry if we are already reconnecting a
    /// session that used to work, otherwise report the error.
    fn on_session_lost(&mut self, e: &EngineError) {
        let auto = self.status.borrow().auto_reconnect;
        if e.kind() == ErrorKind::FirewallBlocked {
            // Retrying cannot help until the user changes the firewall.
            self.fail(e);
            return;
        }
        let retryable = matches!(
            e.kind(),
            ErrorKind::Unreachable | ErrorKind::Protocol | ErrorKind::SpeakerGone
        );
        if self.attempt == 0
            && e.kind() == ErrorKind::Unreachable
            && self.initial_retries < INITIAL_CONNECT_RETRIES
        {
            // First connection got no answer: retry quietly, still "connecting".
            self.initial_retries += 1;
            info!(retry = self.initial_retries, error = %e, "no answer, retrying first connection");
            self.reconnect_at = Some(tokio::time::Instant::now() + INITIAL_RETRY_DELAY);
            self.update(|s| s.state = EngineState::Connecting);
            return;
        }
        if auto && retryable && self.attempt > 0 && self.attempt < MAX_RECONNECT_ATTEMPTS {
            self.attempt += 1;
            self.schedule_reconnect(Duration::from_secs(2u64.pow(self.attempt.min(5))), e);
        } else {
            self.fail(e);
        }
    }

    fn schedule_reconnect(&mut self, delay: Duration, e: &EngineError) {
        info!(attempt = self.attempt, ?delay, "will reconnect");
        self.reconnect_at = Some(tokio::time::Instant::now() + delay);
        let attempt = self.attempt;
        let err = UiError::from(e);
        self.update(|s| {
            s.state = EngineState::Reconnecting { attempt };
            s.error = Some(err);
        });
    }

    fn fail(&mut self, e: &EngineError) {
        error!(error = %e, "giving up");
        self.sink.set_outputs(Vec::new());
        if let Some(mut src) = self.source.take() {
            src.stop();
        }
        let err = UiError::from(e);
        self.update(|s| {
            s.state = EngineState::Error;
            s.error = Some(err);
        });
    }

    async fn on_keepalive(&mut self) {
        if let Some(message) = self.sink.take_error() {
            let e = EngineError::Capture(crate::source::SourceError::Backend(message));
            self.teardown(true).await;
            self.fail(&e);
            return;
        }
        let paused = self.state() == EngineState::Paused;
        let Some(session) = &mut self.session else {
            return;
        };
        let result = if paused {
            Ok(())
        } else {
            session.keepalive().await
        };
        let sink = self.sink.stats();
        let speakers = session.stats().into_iter().map(Into::into).collect();
        self.update(|s| {
            s.diagnostics = Diagnostics {
                buffer_ms: sink.fill_ms,
                drift_ppm: (sink.elastic.ratio - 1.0) * 1e6,
                buffer_underruns: sink.elastic.underruns,
                dropped_chunks: sink.dropped_chunks,
                speakers,
            };
        });
        if let Err(e) = result {
            warn!(error = %e, "session lost");
            self.sink.set_outputs(Vec::new());
            if let Some(session) = self.session.take() {
                session.stop().await;
            }
            if self.status.borrow().auto_reconnect {
                self.attempt = 1;
                self.schedule_reconnect(Duration::from_secs(2), &e);
            } else {
                self.fail(&e);
            }
        }
    }

    async fn teardown(&mut self, stop_source: bool) {
        self.generation += 1;
        self.reconnect_at = None;
        self.sink.begin_generation(self.generation);
        if let Some(session) = self.session.take() {
            session.stop().await;
        }
        if stop_source {
            if let Some(mut src) = self.source.take() {
                src.stop();
            }
        }
    }
}
