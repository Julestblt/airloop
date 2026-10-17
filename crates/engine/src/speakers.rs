//! Turns raw AirPlay advertisements into the speakers a user picks from.
//!
//! The important rule: a HomePod stereo pair configured in the Home app is
//! advertised as two devices that share a `tsid` (tight-sync id). Members of a
//! pair cannot be targeted individually — sending to the pair's leader plays
//! on both, left and right. So a pair becomes one [`Speaker`] of kind
//! [`SpeakerKind::StereoPair`] whose target is the leader.

use airplay_core::Device;
use serde::Serialize;
use std::collections::BTreeMap;
use std::net::IpAddr;

/// What a speaker entry represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SpeakerKind {
    /// One physical AirPlay receiver.
    Single,
    /// A Home-app stereo pair, addressed through its leader.
    StereoPair,
}

/// A logical AirPlay destination shown to the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Speaker {
    /// Stable identifier, used for selection and in saved preferences.
    pub id: String,
    /// Display name, e.g. "Living Room".
    pub name: String,
    /// Raw Apple model identifier, e.g. `AudioAccessory5,1`.
    pub model: String,
    /// Human-readable model, e.g. "HomePod mini".
    pub model_name: String,
    pub kind: SpeakerKind,
    /// Address that will be connected to.
    pub address: Option<String>,
    /// Number of physical receivers behind this entry.
    pub member_count: usize,
    /// `None` when Airloop can stream to it, otherwise why not.
    pub unsupported_reason: Option<String>,
}

/// A speaker plus the device the engine actually connects to.
#[derive(Debug, Clone)]
pub struct Target {
    pub speaker: Speaker,
    pub device: Device,
}

/// Maps Apple model identifiers to marketing names.
pub fn model_name(model: &str) -> String {
    let name = match model {
        "AudioAccessory5,1" => "HomePod mini",
        "AudioAccessory1,1" | "AudioAccessory1,2" => "HomePod",
        "AudioAccessory6,1" => "HomePod (2nd generation)",
        m if m.starts_with("AppleTV") => "Apple TV",
        m if m.starts_with("MacBook") || m.starts_with("Mac") || m.starts_with("iMac") => "Mac",
        "" => "AirPlay speaker",
        m => m,
    };
    name.to_string()
}

/// Prefers an IPv4 address: link-local IPv6 needs a scope id and global IPv6
/// is often filtered on home routers.
pub fn preferred_address(addresses: &[IpAddr]) -> Option<IpAddr> {
    addresses
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addresses.first())
        .copied()
}

fn unsupported_reason(device: &Device) -> Option<String> {
    if !device.supports_airplay2() {
        return Some("Only AirPlay 2 receivers are supported".into());
    }
    if device.requires_password {
        return Some("This speaker requires a password, which Airloop does not support yet".into());
    }
    None
}

fn single(device: &Device) -> Target {
    let speaker = Speaker {
        id: device.id.to_mac_string(),
        name: device.name.clone(),
        model: device.model.clone(),
        model_name: model_name(&device.model),
        kind: SpeakerKind::Single,
        address: preferred_address(&device.addresses).map(|a| a.to_string()),
        member_count: 1,
        unsupported_reason: unsupported_reason(device),
    };
    Target {
        speaker,
        device: device.clone(),
    }
}

/// Picks the member to address in a stereo pair. The leader advertises the
/// pair's public name (`gpn`) as its own service name; the other member gets
/// a disambiguated name such as "Living Room (2)".
fn pair_leader<'a>(members: &[&'a Device]) -> &'a Device {
    members
        .iter()
        .find(|d| d.group_public_name.as_deref() == Some(d.name.as_str()))
        .copied()
        .unwrap_or_else(|| {
            // Deterministic fallback: shortest name, then lowest id.
            members
                .iter()
                .min_by(|a, b| a.name.len().cmp(&b.name.len()).then(a.id.0.cmp(&b.id.0)))
                .copied()
                .unwrap_or(members[0])
        })
}

/// Collapses discovered devices into user-facing speakers, sorted by name.
pub fn group_speakers(devices: &[Device]) -> Vec<Target> {
    let mut by_tsid: BTreeMap<uuid::Uuid, Vec<&Device>> = BTreeMap::new();
    let mut targets = Vec::new();

    for device in devices {
        match device.tight_sync_id {
            Some(tsid) => by_tsid.entry(tsid).or_default().push(device),
            None => targets.push(single(device)),
        }
    }

    for (tsid, members) in by_tsid {
        if members.len() < 2 {
            targets.push(single(members[0]));
            continue;
        }
        let leader = pair_leader(&members);
        let mut target = single(leader);
        target.speaker.id = format!("pair-{tsid}");
        target.speaker.kind = SpeakerKind::StereoPair;
        target.speaker.member_count = members.len();
        if let Some(name) = &leader.group_public_name {
            target.speaker.name = name.clone();
        }
        target.speaker.model_name = format!("{} stereo pair", model_name(&leader.model));
        targets.push(target);
    }

    targets.sort_by(|a, b| {
        a.speaker
            .name
            .to_lowercase()
            .cmp(&b.speaker.name.to_lowercase())
            .then(a.speaker.id.cmp(&b.speaker.id))
    });
    targets
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use airplay_core::{DeviceId, Features};
    use std::net::Ipv4Addr;

    /// Features advertised by a HomePod mini on firmware 27.0 (captured on a
    /// real device).
    const HOMEPOD_FEATURES: &str = "0x4A7FCA00,0x3C354BD0";

    pub(crate) fn homepod(
        mac: u8,
        name: &str,
        ip: u8,
        tsid: Option<&str>,
        gpn: Option<&str>,
    ) -> Device {
        Device {
            id: DeviceId([0x12, 0x4A, 0xC8, 0xD1, 0xF1, mac]),
            name: name.into(),
            model: "AudioAccessory5,1".into(),
            manufacturer: None,
            serial_number: None,
            addresses: vec![
                "fe80::1".parse().expect("valid ip"),
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, ip)),
            ],
            port: 7000,
            features: Features::from_txt_value(HOMEPOD_FEATURES).expect("valid features"),
            required_sender_features: None,
            public_key: None,
            source_version: airplay_core::Version::new(980, 77, 2),
            firmware_version: None,
            os_version: None,
            protocol_version: None,
            requires_password: false,
            status_flags: 0,
            access_control: None,
            pairing_identity: None,
            system_pairing_identity: None,
            bluetooth_address: None,
            homekit_home_id: None,
            group_id: None,
            is_group_leader: false,
            group_public_name: gpn.map(Into::into),
            group_contains_discoverable_leader: false,
            home_group_id: None,
            household_id: None,
            parent_group_id: None,
            parent_group_contains_discoverable_leader: false,
            tight_sync_id: tsid.map(|t| t.parse().expect("valid uuid")),
            raop_port: None,
            raop_encryption_types: None,
            raop_codecs: None,
            raop_transport: None,
            raop_metadata_types: None,
            raop_digest_auth: false,
            vodka_version: None,
        }
    }

    const TSID: &str = "1EFB9D29-C545-531E-A986-766AF173E59D";

    #[test]
    fn stereo_pair_collapses_to_leader() {
        // Mirrors the real advertisement: "Living Room" + "Living Room (2)".
        let devices = vec![
            homepod(2, "Living Room (2)", 15, Some(TSID), Some("Living Room")),
            homepod(1, "Living Room", 14, Some(TSID), Some("Living Room")),
        ];
        let targets = group_speakers(&devices);
        assert_eq!(targets.len(), 1);
        let t = &targets[0];
        assert_eq!(t.speaker.kind, SpeakerKind::StereoPair);
        assert_eq!(t.speaker.name, "Living Room");
        assert_eq!(t.speaker.member_count, 2);
        assert_eq!(t.speaker.address.as_deref(), Some("192.168.1.14"));
        assert_eq!(t.speaker.model_name, "HomePod mini stereo pair");
        assert!(t.speaker.id.starts_with("pair-"));
        assert_eq!(t.speaker.unsupported_reason, None);
    }

    #[test]
    fn lone_tsid_is_single() {
        let devices = vec![homepod(3, "Kitchen", 20, Some(TSID), Some("Kitchen"))];
        let targets = group_speakers(&devices);
        assert_eq!(targets[0].speaker.kind, SpeakerKind::Single);
        assert_eq!(targets[0].speaker.model_name, "HomePod mini");
    }

    #[test]
    fn leader_fallback_when_gpn_missing() {
        let devices = vec![
            homepod(2, "Office (2)", 31, Some(TSID), None),
            homepod(1, "Office", 30, Some(TSID), None),
        ];
        let targets = group_speakers(&devices);
        assert_eq!(targets[0].speaker.address.as_deref(), Some("192.168.1.30"));
    }

    #[test]
    fn sorted_by_name() {
        let devices = vec![
            homepod(1, "bedroom", 10, None, None),
            homepod(2, "Attic", 11, None, None),
        ];
        let names: Vec<_> = group_speakers(&devices)
            .into_iter()
            .map(|t| t.speaker.name)
            .collect();
        assert_eq!(names, ["Attic", "bedroom"]);
    }

    #[test]
    fn password_devices_flagged() {
        let mut d = homepod(1, "Locked", 10, None, None);
        d.requires_password = true;
        assert!(group_speakers(&[d])[0].speaker.unsupported_reason.is_some());
    }

    #[test]
    fn prefers_ipv4() {
        let addrs: Vec<IpAddr> = vec![
            "fe80::1".parse().expect("ip"),
            "10.0.0.2".parse().expect("ip"),
        ];
        assert_eq!(
            preferred_address(&addrs),
            Some("10.0.0.2".parse().expect("ip"))
        );
    }
}
