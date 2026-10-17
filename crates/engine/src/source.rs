//! Audio sources: anything that can push 44.1 kHz stereo PCM into a sink.

use crate::sink::FrameSink;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("not supported on this platform")]
    Unsupported,
    #[error("process {0} is not running")]
    ProcessGone(u32),
    #[error("{0}")]
    Backend(String),
}

/// A running audio producer.
///
/// `start` must spawn its own thread and return quickly; that thread calls
/// [`FrameSink::push`] with interleaved stereo `f32` at [`crate::SAMPLE_RATE`]
/// until `stop` is called. A source outlives individual AirPlay sessions, so
/// reconnecting a speaker does not restart capture.
pub trait AudioSource: Send + 'static {
    /// Human-readable description for logs and the UI.
    fn describe(&self) -> String;
    fn start(&mut self, sink: FrameSink) -> Result<(), SourceError>;
    fn stop(&mut self);
}

/// A sine tone, paced by the system clock. Used for "test sound" and for
/// headless protocol testing on machines without audio capture.
pub struct ToneSource {
    frequency: f32,
    amplitude: f32,
    duration: Option<Duration>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ToneSource {
    /// `amplitude` is linear full-scale (0.0–1.0). `duration` of `None` plays
    /// until stopped.
    pub fn new(frequency: f32, amplitude: f32, duration: Option<Duration>) -> Self {
        Self {
            frequency,
            amplitude: amplitude.clamp(0.0, 1.0),
            duration,
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }
}

impl AudioSource for ToneSource {
    fn describe(&self) -> String {
        format!("{} Hz test tone", self.frequency)
    }

    fn start(&mut self, sink: FrameSink) -> Result<(), SourceError> {
        self.running.store(true, Ordering::Release);
        let running = Arc::clone(&self.running);
        let (freq, amp, duration) = (self.frequency, self.amplitude, self.duration);
        let handle = std::thread::Builder::new()
            .name("tone-source".into())
            .spawn(move || {
                const CHUNK: usize = 441; // 10 ms
                let rate = crate::SAMPLE_RATE as f32;
                let mut phase = 0.0f32;
                let mut buf = vec![0.0f32; CHUNK * 2];
                let start = Instant::now();
                let mut produced: u64 = 0;
                while running.load(Ordering::Acquire) {
                    if duration.is_some_and(|d| start.elapsed() >= d) {
                        break;
                    }
                    for frame in buf.chunks_exact_mut(2) {
                        // Short fade-in avoids a click at the start.
                        let fade = (produced as f32 / 4410.0).min(1.0);
                        let s = (phase * std::f32::consts::TAU).sin() * amp * fade;
                        frame[0] = s;
                        frame[1] = s;
                        phase = (phase + freq / rate).fract();
                        produced += 1;
                    }
                    sink.push(&buf, crate::CHANNELS);
                    // Pace against the wall clock so we never run ahead.
                    let due =
                        Duration::from_secs_f64(produced as f64 / f64::from(crate::SAMPLE_RATE));
                    if let Some(wait) = due.checked_sub(start.elapsed()) {
                        std::thread::sleep(wait);
                    }
                }
                running.store(false, Ordering::Release);
            })
            .map_err(|e| SourceError::Backend(e.to_string()))?;
        self.thread = Some(handle);
        Ok(())
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ToneSource {
    fn drop(&mut self) {
        self.stop();
    }
}
