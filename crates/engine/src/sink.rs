//! The hand-off point between an audio source and the AirPlay session(s).
//!
//! Sources call [`FrameSink::push`] from their own thread. A dedicated clock
//! thread drains an [`ElasticBuffer`] at exactly real-time rate (by the system
//! clock, which is also what paces the AirPlay sender) and fans each 10 ms
//! chunk out to every connected speaker. Outputs can be swapped while running,
//! so a reconnect does not restart capture.

use crate::drift::{ElasticBuffer, ElasticStats};
use airplay_audio::{LiveFrameSender, LivePcmFrame};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Frames per clock tick (10 ms at 44.1 kHz).
const TICK_FRAMES: usize = 441;
/// Target elastic-buffer fill: absorbs WASAPI's bursty delivery.
const TARGET_FRAMES: usize = 2_646; // 60 ms
const CAPACITY_FRAMES: usize = 22_050; // 500 ms

struct Inner {
    buffer: Mutex<ElasticBuffer>,
    outputs: Mutex<Vec<LiveFrameSender>>,
    paused: AtomicBool,
    running: AtomicBool,
    /// Peak level of the last ticks, as f32 bits, for the UI meter.
    peak: AtomicU32,
    /// Chunks the AirPlay side could not accept (its queue was full).
    dropped_chunks: AtomicU64,
    /// Ticks that produced real audio, for diagnostics.
    audio_ticks: AtomicU64,
    /// Session generation allowed to install outputs (see `install_outputs`).
    generation: AtomicU64,
    /// Fatal error reported by the source, picked up by the engine.
    source_error: Mutex<Option<String>>,
}

/// Cloneable handle that sources push audio into.
#[derive(Clone)]
pub struct FrameSink {
    inner: Arc<Inner>,
}

/// Owns the clock thread; dropping it stops the thread.
pub struct SinkClock {
    inner: Arc<Inner>,
    thread: Option<JoinHandle<()>>,
}

/// Snapshot of sink health for diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SinkStats {
    pub elastic: ElasticStats,
    pub fill_ms: f32,
    pub dropped_chunks: u64,
    pub audio_ticks: u64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding these locks cannot leave the data inconsistent in
    // a way that matters for audio, so recover instead of propagating poison.
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl FrameSink {
    /// Creates a sink and starts its real-time clock thread.
    pub fn new() -> (FrameSink, SinkClock) {
        let inner = Arc::new(Inner {
            buffer: Mutex::new(ElasticBuffer::new(TARGET_FRAMES, CAPACITY_FRAMES)),
            outputs: Mutex::new(Vec::new()),
            paused: AtomicBool::new(false),
            running: AtomicBool::new(true),
            peak: AtomicU32::new(0),
            dropped_chunks: AtomicU64::new(0),
            audio_ticks: AtomicU64::new(0),
            source_error: Mutex::new(None),
            generation: AtomicU64::new(0),
        });
        let clock_inner = Arc::clone(&inner);
        let thread = std::thread::Builder::new()
            .name("airloop-clock".into())
            .spawn(move || clock_loop(&clock_inner))
            .ok();
        if thread.is_none() {
            tracing::error!("failed to spawn audio clock thread");
        }
        (
            FrameSink {
                inner: Arc::clone(&inner),
            },
            SinkClock { inner, thread },
        )
    }

    /// Pushes interleaved `f32` samples at 44.1 kHz. Mono is duplicated to
    /// both channels; more than two channels are folded down to stereo.
    pub fn push(&self, interleaved: &[f32], channels: u16) {
        match channels {
            2 => lock(&self.inner.buffer).push(interleaved),
            0 => {}
            1 => {
                let stereo: Vec<f32> = interleaved.iter().flat_map(|s| [*s, *s]).collect();
                lock(&self.inner.buffer).push(&stereo);
            }
            n => {
                let n = usize::from(n);
                let stereo: Vec<f32> = interleaved.chunks_exact(n).flat_map(downmix).collect();
                lock(&self.inner.buffer).push(&stereo);
            }
        }
    }

    /// Replaces the AirPlay outputs. An empty list keeps the clock running and
    /// discards audio.
    pub fn set_outputs(&self, outputs: Vec<LiveFrameSender>) {
        *lock(&self.inner.outputs) = outputs;
    }

    /// Marks `generation` as the only session allowed to install outputs and
    /// clears the current ones.
    pub fn begin_generation(&self, generation: u64) {
        let mut outputs = lock(&self.inner.outputs);
        self.inner.generation.store(generation, Ordering::Release);
        outputs.clear();
    }

    /// Installs outputs for a session, unless a newer session has started
    /// meanwhile. Returns whether they were installed.
    pub fn install_outputs(&self, generation: u64, outputs: Vec<LiveFrameSender>) -> bool {
        let mut current = lock(&self.inner.outputs);
        if self.inner.generation.load(Ordering::Acquire) != generation {
            return false;
        }
        *current = outputs;
        true
    }

    pub fn set_paused(&self, paused: bool) {
        self.inner.paused.store(paused, Ordering::Release);
        if paused {
            lock(&self.inner.buffer).clear();
        }
    }

    /// Called by a source that cannot continue (device gone, process
    /// exited). The engine stops the session and shows the message.
    pub fn report_error(&self, message: impl Into<String>) {
        *lock(&self.inner.source_error) = Some(message.into());
    }

    /// Takes the pending source error, if any.
    pub fn take_error(&self) -> Option<String> {
        lock(&self.inner.source_error).take()
    }

    /// Peak level (0.0–1.0) since the last call, for a level meter.
    pub fn take_peak(&self) -> f32 {
        f32::from_bits(self.inner.peak.swap(0, Ordering::AcqRel))
    }

    pub fn stats(&self) -> SinkStats {
        let b = lock(&self.inner.buffer);
        SinkStats {
            elastic: b.stats(),
            fill_ms: b.fill() as f32 * 1000.0 / crate::SAMPLE_RATE as f32,
            dropped_chunks: self.inner.dropped_chunks.load(Ordering::Relaxed),
            audio_ticks: self.inner.audio_ticks.load(Ordering::Relaxed),
        }
    }
}

/// ITU-style fold-down of front L/R, centre and surrounds to stereo.
/// Channel order follows WAVEFORMATEXTENSIBLE: FL FR FC LFE BL BR SL SR.
fn downmix(f: &[f32]) -> [f32; 2] {
    let at = |i: usize| f.get(i).copied().unwrap_or(0.0);
    const C: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let l = at(0) + C * at(2) + C * at(4) + C * at(6);
    let r = at(1) + C * at(2) + C * at(5) + C * at(7);
    // Normalise so a full-scale signal on every channel cannot clip hard.
    let norm = 1.0 / (1.0 + 3.0 * C);
    [l * norm, r * norm]
}

fn clock_loop(inner: &Inner) {
    let tick = Duration::from_secs_f64(TICK_FRAMES as f64 / f64::from(crate::SAMPLE_RATE));
    let mut out = vec![0.0f32; TICK_FRAMES * 2];
    let start = Instant::now();
    let mut ticks: u32 = 0;
    while inner.running.load(Ordering::Acquire) {
        ticks = ticks.wrapping_add(1);
        let deadline = start + tick * ticks;
        if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }

        let paused = inner.paused.load(Ordering::Acquire);
        let has_audio = !paused && lock(&inner.buffer).pull(&mut out);
        if !has_audio {
            out.fill(0.0);
        } else {
            inner.audio_ticks.fetch_add(1, Ordering::Relaxed);
        }

        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).min(1.0);
        // For non-negative floats the IEEE-754 bit pattern orders like the
        // value, so an integer max is a float max.
        inner.peak.fetch_max(peak.to_bits(), Ordering::AcqRel);

        if paused {
            continue;
        }
        let outputs = lock(&inner.outputs);
        if outputs.is_empty() {
            continue;
        }
        // Silence is still sent: AirPlay expects a continuous stream, and a
        // gap would make the receiver's buffer run dry.
        let samples: Vec<i16> = out.iter().map(|s| to_i16(*s)).collect();
        for o in outputs.iter() {
            let frame = LivePcmFrame {
                samples: samples.clone(),
                channels: 2,
                sample_rate: crate::SAMPLE_RATE,
            };
            if !o.try_send(frame) {
                inner.dropped_chunks.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

impl Drop for SinkClock {
    fn drop(&mut self) {
        self.inner.running.store(false, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_conversion_clamps() {
        assert_eq!(to_i16(2.0), 32767);
        assert_eq!(to_i16(-2.0), -32767);
        assert_eq!(to_i16(0.0), 0);
    }

    #[test]
    fn downmix_51_centre_goes_to_both() {
        let [l, r] = downmix(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        assert!(l > 0.0 && (l - r).abs() < 1e-6);
    }

    #[test]
    fn fans_out_to_outputs_in_real_time() {
        let (sink, _clock) = FrameSink::new();
        let (tx, mut rx) = airplay_audio::LiveAudioDecoder::create_pair(44_100, 2, 64);
        sink.set_outputs(vec![tx]);
        let tone: Vec<f32> = (0..44_100)
            .flat_map(|i| {
                let s = (i as f32 * 0.05).sin() * 0.5;
                [s, s]
            })
            .collect();
        sink.push(&tone, 2);
        std::thread::sleep(Duration::from_millis(250));
        let mut frames = 0;
        let mut loud = false;
        while let Ok(Some(f)) = rx.decode_frame() {
            frames += f.samples.len() / 2;
            loud |= f.samples.iter().any(|s| s.abs() > 1000);
        }
        // ~250 ms worth of ticks, allowing for scheduler slop.
        assert!((8_000..=13_000).contains(&frames), "frames {frames}");
        assert!(loud);
        assert!(sink.take_peak() > 0.4);
    }
}
