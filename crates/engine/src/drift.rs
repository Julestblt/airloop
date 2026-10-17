//! Clock-domain bridge between an audio source and the AirPlay sender.
//!
//! The source (WASAPI loopback) runs on the sound card's clock and delivers
//! audio in bursts, and stops delivering entirely while nothing is playing.
//! The AirPlay sender is paced by the system clock. Left alone, the two drift
//! apart by tens of ppm, which eventually means an overflowing or empty buffer
//! and an audible glitch every few minutes.
//!
//! [`ElasticBuffer`] absorbs that: the producer appends whatever it has, the
//! consumer asks for exactly N frames per tick, and the buffer keeps its fill
//! level near a target by resampling a tiny amount (at most ±0.3%, inaudible)
//! with cubic interpolation. When the producer starves it outputs silence and
//! refills to the target before playing again, so a silent source becomes
//! clean silence instead of crackle.

use std::collections::VecDeque;

/// Interleaved stereo frame buffer with adaptive-rate readout.
pub struct ElasticBuffer {
    samples: VecDeque<f32>,
    /// Target fill in frames.
    target: usize,
    /// Hard cap in frames; older audio is discarded beyond it.
    capacity: usize,
    /// Fractional read position within the first frame (0.0..1.0).
    frac: f64,
    /// Smoothed fill level, in frames.
    fill_ema: f64,
    /// Whether we are refilling after starvation.
    priming: bool,
    stats: ElasticStats,
}

/// Counters for diagnostics.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ElasticStats {
    /// Times the buffer ran dry while playing.
    pub underruns: u64,
    /// Frames discarded because the producer ran too far ahead.
    pub overflow_frames: u64,
    /// Current playback ratio (input frames consumed per output frame).
    pub ratio: f64,
}

const CHANNELS: usize = 2;
/// Maximum correction applied, as a fraction of the nominal rate.
const MAX_CORRECTION: f64 = 0.003;
/// Proportional gain: correction per unit of relative fill error.
const GAIN: f64 = 0.002;

impl ElasticBuffer {
    pub fn new(target_frames: usize, capacity_frames: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity_frames * CHANNELS),
            target: target_frames.max(1),
            capacity: capacity_frames.max(target_frames * 2),
            frac: 0.0,
            fill_ema: target_frames as f64,
            priming: true,
            stats: ElasticStats {
                ratio: 1.0,
                ..Default::default()
            },
        }
    }

    /// Frames currently buffered.
    pub fn fill(&self) -> usize {
        self.samples.len() / CHANNELS
    }

    pub fn stats(&self) -> ElasticStats {
        self.stats
    }

    /// Drops all buffered audio and waits for a fresh refill.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.frac = 0.0;
        self.priming = true;
    }

    /// Appends interleaved stereo samples.
    pub fn push(&mut self, interleaved: &[f32]) {
        self.samples.extend(interleaved);
        let excess = self.fill().saturating_sub(self.capacity);
        if excess > 0 {
            self.samples.drain(..excess * CHANNELS);
            self.stats.overflow_frames += excess as u64;
        }
    }

    fn frame(&self, i: usize) -> (f32, f32) {
        let n = self.fill();
        let i = i.min(n.saturating_sub(1));
        (self.samples[i * CHANNELS], self.samples[i * CHANNELS + 1])
    }

    /// Fills `out` (interleaved stereo) with exactly `out.len() / 2` frames.
    /// Returns `false` if the output is silence because the buffer is priming.
    pub fn pull(&mut self, out: &mut [f32]) -> bool {
        let frames = out.len() / CHANNELS;

        if self.priming {
            if self.fill() < self.target {
                out.fill(0.0);
                return false;
            }
            self.priming = false;
            self.fill_ema = self.fill() as f64;
        }

        // Need 2 frames of lookahead for cubic interpolation.
        let needed = (frames as f64 * (1.0 + MAX_CORRECTION)).ceil() as usize + 3;
        if self.fill() < needed {
            // Starved: play what is left would mean a partial chunk followed
            // by a hard cut. Emit silence and prime again instead.
            self.stats.underruns += 1;
            self.clear();
            out.fill(0.0);
            return false;
        }

        self.fill_ema += (self.fill() as f64 - self.fill_ema) * 0.01;
        let error = (self.fill_ema - self.target as f64) / self.target as f64;
        let ratio = 1.0 + (error * GAIN).clamp(-MAX_CORRECTION, MAX_CORRECTION);
        self.stats.ratio = ratio;

        let mut pos = self.frac;
        for o in out.chunks_exact_mut(CHANNELS) {
            let i = pos.floor() as usize;
            let t = (pos - i as f64) as f32;
            let p0 = if i == 0 {
                self.frame(0)
            } else {
                self.frame(i - 1)
            };
            let p1 = self.frame(i);
            let p2 = self.frame(i + 1);
            let p3 = self.frame(i + 2);
            o[0] = hermite(p0.0, p1.0, p2.0, p3.0, t);
            o[1] = hermite(p0.1, p1.1, p2.1, p3.1, t);
            pos += ratio;
        }
        let consumed = pos.floor() as usize;
        self.frac = pos - consumed as f64;
        // Keep one frame of history so the next chunk's p0 is real audio.
        let drop = consumed.saturating_sub(1).min(self.fill());
        self.samples.drain(..drop * CHANNELS);
        if consumed >= 1 {
            self.frac += 1.0;
        }
        true
    }
}

/// 4-point Catmull-Rom (cubic Hermite) interpolation between `p1` and `p2`.
fn hermite(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let c0 = p1;
    let c1 = 0.5 * (p2 - p0);
    let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
    ((c3 * t + c2) * t + c1) * t + c0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(start: usize, frames: usize) -> Vec<f32> {
        (start..start + frames)
            .flat_map(|i| [i as f32, -(i as f32)])
            .collect()
    }

    #[test]
    fn primes_with_silence() {
        let mut b = ElasticBuffer::new(100, 1000);
        b.push(&ramp(0, 50));
        let mut out = vec![1.0; 20];
        assert!(!b.pull(&mut out));
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn passes_audio_through_at_unity() {
        let mut b = ElasticBuffer::new(100, 1000);
        b.push(&ramp(0, 100));
        let mut out = vec![0.0; 20];
        assert!(b.pull(&mut out));
        // At target fill the ratio is 1.0 and the ramp is reproduced exactly
        // (cubic interpolation of a straight line is exact).
        let expected = ramp(0, 10);
        for (a, e) in out.iter().zip(&expected) {
            assert!((a - e).abs() < 1e-3, "{a} vs {e}");
        }
    }

    #[test]
    fn continuous_across_chunks() {
        let mut b = ElasticBuffer::new(100, 1000);
        b.push(&ramp(0, 400));
        let mut out = Vec::new();
        for _ in 0..10 {
            let mut chunk = vec![0.0; 20];
            assert!(b.pull(&mut chunk));
            out.extend(chunk.chunks(2).map(|f| f[0]));
        }
        // Monotonic, steps close to 1 (slight speed-up because over target).
        for w in out.windows(2) {
            let step = w[1] - w[0];
            assert!(step > 0.99 && step < 1.01, "step {step}");
        }
    }

    #[test]
    fn underrun_emits_silence_and_reprimes() {
        let mut b = ElasticBuffer::new(10, 1000);
        b.push(&ramp(0, 12));
        let mut out = vec![0.0; 40]; // 20 frames > 12 available
        assert!(!b.pull(&mut out));
        assert_eq!(b.stats().underruns, 1);
        assert_eq!(b.fill(), 0);
    }

    #[test]
    fn overflow_discards_oldest() {
        let mut b = ElasticBuffer::new(10, 50);
        b.push(&ramp(0, 80));
        assert_eq!(b.fill(), 50);
        assert_eq!(b.stats().overflow_frames, 30);
    }

    /// Simulates a source 200 ppm fast feeding a consumer for 10 minutes:
    /// the fill level must stay bounded with no underruns or overflow.
    #[test]
    fn absorbs_clock_drift() {
        let target = 2646; // 60 ms
        let mut b = ElasticBuffer::new(target, target * 4);
        let mut produced = 0.0f64;
        let mut out = vec![0.0; 441 * 2];
        let src_rate = 44_100.0 * 1.0002;
        let mut next = 0usize;
        for tick in 0..60_000 {
            // Source delivers in 10 ms bursts of its own clock.
            produced += src_rate / 100.0;
            let whole = produced as usize - next;
            b.push(&ramp(next, whole));
            next += whole;
            b.pull(&mut out);
            if tick > 6000 {
                assert!(b.fill() < target * 2, "fill {} at tick {tick}", b.fill());
            }
        }
        let s = b.stats();
        assert!(s.underruns <= 1, "{s:?}"); // only the initial priming
        assert_eq!(s.overflow_frames, 0);
        assert!(s.ratio > 1.0001 && s.ratio < 1.0004, "{s:?}");
    }
}
