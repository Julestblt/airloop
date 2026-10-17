//! Volume mapping between the UI scale and AirPlay.
//!
//! AirPlay carries volume in dB: `-30.0..=0.0`, plus `-144.0` for mute. The UI
//! uses a 0–100 slider mapped linearly onto that dB range, which matches how
//! Apple senders behave (each step is a constant loudness ratio).
//!
//! `airplay2-rs` takes a linear gain in `0.0..=1.0` and sends `20·log10(gain)`
//! dB, so we convert our dB target back into that linear value.

/// Quietest audible AirPlay volume in dB.
pub const MIN_DB: f32 = -30.0;

/// Default volume for a brand-new speaker. Deliberately low: a sender that
/// starts at 0 dB on a HomePod is painfully loud.
pub const DEFAULT_PERCENT: u8 = 25;

/// Converts a UI percentage to AirPlay dB. `None` means mute.
pub fn percent_to_db(percent: u8) -> Option<f32> {
    match percent.min(100) {
        0 => None,
        p => Some(MIN_DB * (1.0 - f32::from(p) / 100.0)),
    }
}

/// Converts a UI percentage to the linear gain `airplay2-rs` expects.
pub fn percent_to_client_gain(percent: u8) -> f32 {
    match percent_to_db(percent) {
        None => 0.0,
        Some(db) => 10f32.powf(db / 20.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_mute() {
        assert_eq!(percent_to_db(0), None);
        assert_eq!(percent_to_client_gain(0), 0.0);
    }

    #[test]
    fn full_is_zero_db() {
        assert_eq!(percent_to_db(100), Some(0.0));
        assert!((percent_to_client_gain(100) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn values_above_100_clamp() {
        assert_eq!(percent_to_db(250), Some(0.0));
    }

    #[test]
    fn gain_round_trips_to_db() {
        for p in 1..=100u8 {
            let db = percent_to_db(p).unwrap_or(-144.0);
            let gain = percent_to_client_gain(p);
            assert!((20.0 * gain.log10() - db).abs() < 1e-3, "p={p}");
            assert!((MIN_DB..=0.0).contains(&db));
        }
    }

    #[test]
    fn default_is_quiet() {
        let db = percent_to_db(DEFAULT_PERCENT).unwrap_or(0.0);
        assert!(db <= -20.0);
    }
}
