//! Airloop streaming engine.
//!
//! Platform-independent: it knows how to find AirPlay 2 speakers, open an
//! AirPlay session to one or more of them, and stream 44.1 kHz stereo PCM that
//! an [`AudioSource`] pushes into a [`FrameSink`]. Audio capture itself lives
//! in `airloop-capture`; the UI lives in the Tauri app.

pub mod controller;
pub mod discovery;
pub mod drift;
pub mod error;
pub mod session;
pub mod sink;
pub mod source;
pub mod speakers;
pub mod volume;

pub use controller::{Engine, EngineState, EngineStatus};
pub use discovery::SpeakerDiscovery;
pub use error::{EngineError, ErrorKind};
pub use sink::FrameSink;
pub use source::{AudioSource, SourceError, ToneSource};
pub use speakers::{Speaker, SpeakerKind};

/// Sample rate of the AirPlay realtime ALAC stream. Sources deliver this rate.
pub const SAMPLE_RATE: u32 = 44_100;
/// Channel count of the AirPlay stream. Sources deliver interleaved stereo.
pub const CHANNELS: u16 = 2;
