//! Engine errors, classified so the UI can show a helpful message.

use serde::Serialize;

/// Broad category of a failure, used to pick a user-facing explanation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    /// The speaker could not be reached over the network.
    Unreachable,
    /// The speaker refused pairing (usually Home app access restrictions).
    AccessDenied,
    /// Another program holds the PTP clock ports (UDP 319/320).
    TimingPortsBusy,
    /// The speaker never reached this PC (inbound UDP blocked, usually by
    /// Windows Firewall or a Public network profile).
    FirewallBlocked,
    /// The speaker answered but the AirPlay session could not be set up.
    Protocol,
    /// The selected speaker is no longer on the network.
    SpeakerGone,
    /// Audio capture failed.
    Capture,
    /// Anything else.
    Internal,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("speaker not found: {0}")]
    SpeakerNotFound(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("connecting to {speaker}: {source}")]
    Connect {
        speaker: String,
        #[source]
        source: airplay_core::error::Error,
    },
    #[error("audio capture: {0}")]
    Capture(#[from] crate::source::SourceError),
    #[error("timed out after {0:?} while {1}")]
    Timeout(std::time::Duration, &'static str),
    #[error("{speaker} could not reach this PC: no timing requests arrived (inbound UDP blocked)")]
    InboundBlocked { speaker: String },
    #[error("mDNS discovery: {0}")]
    Discovery(String),
    #[error("{0}")]
    Other(String),
}

impl EngineError {
    /// Classifies the error for the UI.
    pub fn kind(&self) -> ErrorKind {
        match self {
            EngineError::SpeakerNotFound(_) => ErrorKind::SpeakerGone,
            EngineError::Unsupported(_) => ErrorKind::Protocol,
            EngineError::Capture(_) => ErrorKind::Capture,
            EngineError::Timeout(..) => ErrorKind::Unreachable,
            EngineError::InboundBlocked { .. } => ErrorKind::FirewallBlocked,
            EngineError::Discovery(_) | EngineError::Other(_) => ErrorKind::Internal,
            EngineError::Connect { source, .. } => classify(source),
        }
    }

    /// A short sentence suitable for the UI. Never contains secrets.
    pub fn user_message(&self) -> String {
        match self.kind() {
            ErrorKind::Unreachable => {
                "The speaker didn't answer in time. It may still be finishing a previous session: wait a few seconds and try again. If it keeps happening, check that it's on the same network as this PC and that Windows Firewall allows Airloop (network set to Private).".into()
            }
            ErrorKind::AccessDenied => "The speaker refused the connection. In the Home app, set \"Allow Speaker & TV Access\" to \"Everyone\" or \"Anyone on the Same Network\".".into(),
            ErrorKind::FirewallBlocked => "Windows Firewall is blocking the speaker from talking back to this PC. Click \"Allow in firewall\" below, or set your network to Private in Windows settings.".into(),
            ErrorKind::TimingPortsBusy => "Another app is using the AirPlay clock ports (UDP 319/320). Close other AirPlay or PTP software and try again.".into(),
            ErrorKind::SpeakerGone => "The speaker is no longer on the network.".into(),
            ErrorKind::Capture => format!("Couldn't capture audio: {self}"),
            ErrorKind::Protocol | ErrorKind::Internal => format!("Streaming failed: {self}"),
        }
    }
}

fn classify(e: &airplay_core::error::Error) -> ErrorKind {
    use airplay_core::error::{Error, RtspError};
    match e {
        Error::Connection(io) => match io.kind() {
            std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied => {
                ErrorKind::TimingPortsBusy
            }
            _ => ErrorKind::Unreachable,
        },
        Error::Rtsp(RtspError::ConnectionRefused) | Error::Timeout => ErrorKind::Unreachable,
        Error::Rtsp(RtspError::UnexpectedStatus(401 | 403 | 470)) => ErrorKind::AccessDenied,
        Error::Pairing(_) => ErrorKind::AccessDenied,
        _ => {
            let text = e.to_string();
            if text.contains("319") || text.contains("320") {
                ErrorKind::TimingPortsBusy
            } else {
                ErrorKind::Protocol
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airplay_core::error::{Error, RtspError};

    fn connect(source: Error) -> EngineError {
        EngineError::Connect {
            speaker: "x".into(),
            source,
        }
    }

    #[test]
    fn classifies_access_denied() {
        let e = connect(Error::Rtsp(RtspError::UnexpectedStatus(470)));
        assert_eq!(e.kind(), ErrorKind::AccessDenied);
        assert!(e.user_message().contains("Home app"));
    }

    #[test]
    fn classifies_unreachable() {
        let io = std::io::Error::from(std::io::ErrorKind::TimedOut);
        assert_eq!(
            connect(Error::Connection(io)).kind(),
            ErrorKind::Unreachable
        );
    }

    #[test]
    fn rtsp_timeout_is_unreachable() {
        assert_eq!(connect(Error::Timeout).kind(), ErrorKind::Unreachable);
    }

    #[test]
    fn inbound_blocked_is_firewall() {
        let e = EngineError::InboundBlocked {
            speaker: "x".into(),
        };
        assert_eq!(e.kind(), ErrorKind::FirewallBlocked);
        assert!(e.user_message().contains("Firewall"));
    }

    #[test]
    fn classifies_port_busy() {
        let io = std::io::Error::from(std::io::ErrorKind::AddrInUse);
        assert_eq!(
            connect(Error::Connection(io)).kind(),
            ErrorKind::TimingPortsBusy
        );
    }
}
