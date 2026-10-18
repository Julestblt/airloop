//! One AirPlay streaming session to one or more speakers.
//!
//! Single speaker: transient HomeKit pairing → RTSP SETUP (PTP, BMCA yield to
//! the receiver's clock) → realtime ALAC.
//!
//! Several speakers: the first one is set up normally and becomes the PTP
//! reference; the others are set up against the same clock (`setup_for_group`)
//! and every receiver is told about its peers with SETPEERS, which is what lets
//! them play in sync.

use crate::error::EngineError;
use crate::speakers::Target;
use crate::volume;
use airplay_audio::{AlacEncoder, LiveAudioDecoder, LiveFrameSender};
use airplay_client::{Connection, PlaybackState};
use airplay_core::stream::{PtpMode, StreamType, TimingProtocol};
use airplay_core::{AudioCodec, AudioFormat, StreamConfig};
use std::time::Duration;
use tracing::{info, warn};

/// HomeKit transient pairing PIN. AirPlay 2 receivers whose access control
/// allows "everyone on the network" accept this fixed value.
const TRANSIENT_PIN: &str = "3939";
/// Queue between the clock thread and each AirPlay streamer, in 10 ms chunks.
const LIVE_QUEUE_CHUNKS: usize = 64;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const SETUP_TIMEOUT: Duration = Duration::from_secs(20);

/// Per-speaker counters, for diagnostics.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerStats {
    pub speaker_id: String,
    pub packets_sent: u64,
    pub underruns: u64,
}

struct Member {
    target: Target,
    conn: Connection,
}

/// A live session. Dropping it without calling [`StreamSession::stop`] leaves
/// sockets to be closed by the OS; always stop explicitly when possible.
pub struct StreamSession {
    members: Vec<Member>,
    feedback_failures: u32,
}

fn stream_config(timing: TimingProtocol) -> Result<StreamConfig, EngineError> {
    let audio_format = AudioFormat::default();
    debug_assert_eq!(audio_format.codec, AudioCodec::Alac);
    let cookie = AlacEncoder::new(audio_format)
        .map_err(|e| EngineError::Other(format!("ALAC encoder: {e}")))?
        .magic_cookie();
    Ok(StreamConfig {
        stream_type: StreamType::Realtime,
        audio_format,
        timing_protocol: timing,
        ptp_mode: PtpMode::Master,
        latency_min: 22_050,
        latency_max: 88_200,
        supports_dynamic_stream_id: true,
        asc: Some(cookie),
    })
}

async fn timed<T>(
    what: &'static str,
    limit: Duration,
    fut: impl std::future::Future<Output = Result<T, EngineError>>,
) -> Result<T, EngineError> {
    tokio::time::timeout(limit, fut)
        .await
        .map_err(|_| EngineError::Timeout(limit, what))?
}

async fn connect(target: &Target, config: StreamConfig) -> Result<Connection, EngineError> {
    let name = target.speaker.name.clone();
    timed("connecting", CONNECT_TIMEOUT, async {
        Connection::connect_with_pin(target.device.clone(), config, TRANSIENT_PIN)
            .await
            .map_err(|source| EngineError::Connect {
                speaker: name,
                source,
            })
    })
    .await
}

impl StreamSession {
    /// Connects to every target and starts streaming. Returns the session and
    /// one PCM sender per speaker, to be installed in the [`crate::FrameSink`].
    ///
    /// The PCM senders are installed into `sink` *before* streaming starts:
    /// the streamer pre-buffers about a second of audio before sending its
    /// first packet, and receivers drop a session that stays silent.
    pub async fn start(
        targets: Vec<Target>,
        volume_percent: u8,
        sink: &crate::FrameSink,
        generation: u64,
    ) -> Result<StreamSession, EngineError> {
        let Some(first) = targets.first() else {
            return Err(EngineError::Other("no speakers selected".into()));
        };
        for t in &targets {
            if let Some(reason) = &t.speaker.unsupported_reason {
                return Err(EngineError::Unsupported(format!(
                    "{}: {reason}",
                    t.speaker.name
                )));
            }
        }
        // NTP timing: the receivers sync to our clock. Verified audible and
        // stable on HomePod mini (audioOS 27.0); PTP sessions connected but
        // produced no audible output in testing. NTP also needs no privileged
        // ports. `AIRLOOP_TIMING=ptp` re-enables PTP for experiments.
        let timing = match std::env::var("AIRLOOP_TIMING").as_deref() {
            Ok("ptp") => TimingProtocol::Ptp,
            _ => TimingProtocol::Ntp,
        };
        let config = stream_config(timing)?;
        let gain = volume::percent_to_client_gain(volume_percent);
        info!(
            speakers = targets.len(),
            first = %first.speaker.name,
            ?timing,
            "starting AirPlay session"
        );

        let started = std::time::Instant::now();
        let mut members = Vec::with_capacity(targets.len());
        for target in targets {
            info!(speaker = %target.speaker.name, address = ?target.speaker.address, "step: connect and pair");
            let mut conn = connect(&target, config.clone()).await?;
            conn.set_initial_volume(gain);
            members.push(Member { target, conn });
        }

        let mut session = StreamSession {
            members,
            feedback_failures: 0,
        };
        info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "step: RTSP setup"
        );
        if let Err(e) = session.setup_all().await {
            // With NTP timing the receiver must query our timing server
            // before it answers SETUP. If it timed out and not a single
            // timing request arrived, inbound UDP is being blocked.
            let blocked = session
                .members
                .iter()
                .find(|m| m.conn.ntp_requests_received() == Some(0));
            let e = match (&e, blocked) {
                (
                    EngineError::Connect {
                        source: airplay_core::Error::Timeout,
                        ..
                    },
                    Some(m),
                )
                | (EngineError::Timeout(..), Some(m)) => {
                    warn!(speaker = %m.target.speaker.name, "no NTP timing requests received; inbound UDP looks blocked");
                    EngineError::InboundBlocked {
                        speaker: m.target.speaker.name.clone(),
                    }
                }
                _ => e,
            };
            session.stop().await;
            return Err(e);
        }

        let mut senders: Vec<LiveFrameSender> = Vec::with_capacity(session.members.len());
        let mut decoders = Vec::with_capacity(session.members.len());
        for _ in &session.members {
            let (tx, decoder) = LiveAudioDecoder::create_pair(
                crate::SAMPLE_RATE,
                crate::CHANNELS as u8,
                LIVE_QUEUE_CHUNKS,
            );
            senders.push(tx);
            decoders.push(decoder);
        }
        if !sink.install_outputs(generation, senders) {
            session.stop().await;
            return Err(EngineError::Other(
                "superseded by a newer connection".into(),
            ));
        }
        info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "step: start audio"
        );
        for (m, decoder) in session.members.iter_mut().zip(decoders) {
            let name = m.target.speaker.name.clone();
            if let Err(source) = m.conn.start_streaming_live(decoder).await {
                sink.install_outputs(generation, Vec::new());
                session.stop().await;
                return Err(EngineError::Connect {
                    speaker: name,
                    source,
                });
            }
        }
        info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "AirPlay session streaming"
        );
        Ok(session)
    }

    async fn setup_all(&mut self) -> Result<(), EngineError> {
        let wrap = |m: &Member| {
            let speaker = m.target.speaker.name.clone();
            move |source| EngineError::Connect { speaker, source }
        };

        let (first, rest) = self
            .members
            .split_first_mut()
            .ok_or_else(|| EngineError::Other("empty session".into()))?;
        let err = wrap(first);
        timed("setting up the stream", SETUP_TIMEOUT, async {
            first.conn.setup().await.map_err(err)
        })
        .await?;

        if rest.is_empty() {
            return Ok(());
        }

        // Group: share the first receiver's PTP clock with the others.
        let mut peers: Vec<String> = Vec::new();
        for m in std::iter::once(&*first).chain(rest.iter()) {
            if let Some(addr) = crate::speakers::preferred_address(&m.target.device.addresses) {
                peers.push(addr.to_string());
            }
        }
        if let Some(local) = first.conn.local_addr() {
            peers.push(local.ip().to_string());
        }
        let err = wrap(first);
        first.conn.send_setpeers(&peers).await.map_err(err)?;

        let clock_id = first
            .conn
            .ptp_master_clock_id()
            .ok_or_else(|| EngineError::Other("PTP clock negotiation failed".into()))?;
        let offset = first
            .conn
            .timing_offset()
            .ok_or_else(|| EngineError::Other("no PTP timing offset".into()))?;
        for m in rest.iter_mut() {
            let rx = first
                .conn
                .timing_rx()
                .ok_or_else(|| EngineError::Other("no PTP timing channel".into()))?;
            let err = wrap(m);
            timed("joining the speaker group", SETUP_TIMEOUT, async {
                m.conn
                    .setup_for_group(clock_id, offset, rx)
                    .await
                    .map_err(err)
            })
            .await?;
            let err = wrap(m);
            m.conn.send_setpeers(&peers).await.map_err(err)?;
        }
        Ok(())
    }

    /// Names of the speakers in this session.
    pub fn speaker_ids(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|m| m.target.speaker.id.clone())
            .collect()
    }

    /// Sends the periodic keep-alive AirPlay receivers expect (~every 2 s).
    /// Returns an error once the receivers have stopped answering for a while.
    pub async fn keepalive(&mut self) -> Result<(), EngineError> {
        let mut failed = None;
        for m in &mut self.members {
            if let Err(e) = m.conn.send_feedback().await {
                warn!(speaker = %m.target.speaker.name, error = %e, "feedback failed");
                failed = Some((m.target.speaker.name.clone(), e));
            } else if m.conn.playback_state() == PlaybackState::Error {
                failed = Some((
                    m.target.speaker.name.clone(),
                    airplay_core::Error::Streaming(airplay_core::StreamingError::Encoding(
                        "audio streamer stopped".into(),
                    )),
                ));
            }
        }
        match failed {
            None => {
                self.feedback_failures = 0;
                Ok(())
            }
            Some((speaker, source)) => {
                self.feedback_failures += 1;
                // Tolerate one miss (Wi-Fi hiccup), fail on the second.
                if self.feedback_failures >= 2 {
                    Err(EngineError::Connect { speaker, source })
                } else {
                    Ok(())
                }
            }
        }
    }

    pub async fn set_volume(&mut self, percent: u8) {
        let gain = volume::percent_to_client_gain(percent);
        for m in &mut self.members {
            if let Err(e) = m.conn.set_volume(gain).await {
                warn!(speaker = %m.target.speaker.name, error = %e, "set volume failed");
            }
        }
    }

    pub async fn pause(&mut self) {
        for m in &mut self.members {
            if let Err(e) = m.conn.pause().await {
                warn!(speaker = %m.target.speaker.name, error = %e, "pause failed");
            }
        }
    }

    pub async fn resume(&mut self) {
        for m in &mut self.members {
            if let Err(e) = m.conn.resume().await {
                warn!(speaker = %m.target.speaker.name, error = %e, "resume failed");
            }
        }
    }

    pub fn stats(&self) -> Vec<SpeakerStats> {
        self.members
            .iter()
            .map(|m| SpeakerStats {
                speaker_id: m.target.speaker.id.clone(),
                packets_sent: m.conn.streamer_packets_sent(),
                underruns: m.conn.streamer_underruns(),
            })
            .collect()
    }

    /// Stops streaming and tears the RTSP sessions down.
    pub async fn stop(mut self) {
        for m in &mut self.members {
            let name = m.target.speaker.name.clone();
            let res = tokio::time::timeout(Duration::from_secs(3), async {
                if let Err(e) = m.conn.stop().await {
                    warn!(speaker = %name, error = %e, "stop failed");
                }
                if let Err(e) = m.conn.disconnect().await {
                    warn!(speaker = %name, error = %e, "disconnect failed");
                }
            })
            .await;
            if res.is_err() {
                warn!(speaker = %name, "teardown timed out");
            }
        }
        info!("AirPlay session stopped");
    }
}
