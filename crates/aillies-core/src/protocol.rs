//! Versioned device-to-device protocol (carried over pinned mutual TLS).
//!
//! Peer endpoints (HTTP/1.1 inside TLS):
//! - `POST /v1/pair`            unknown client cert + one-time invite secret
//! - `GET  /v1/pair/{id}`       poll approval (same client cert)
//! - `GET  /v1/status`          approved devices only
//! - `POST /v1/codex/responses` approved devices only; streams Responses SSE

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: u32 = 1;
pub const DEFAULT_PEER_PORT: u16 = 18544;
pub const DEFAULT_LOOPBACK_PORT: u16 = 18522;
pub const INVITE_PREFIX: &str = "aillies1:";
pub const INVITE_TTL_SECS: u64 = 600;
pub const PAIR_HEADER_VERSION: &str = "x-aillies-protocol";

/// Account providers. Codex ships first; Claude is reserved for a later release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Text,
    Reasoning,
    ImageGeneration,
    Streaming,
    Cancellation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AccountReadiness {
    Ready,
    Failed { code: crate::FailureCode },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub capabilities: Vec<Capability>,
    pub account: AccountReadiness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatus {
    pub protocol: u32,
    pub app_version: String,
    pub name: String,
    pub platform: String,
    pub sharing: bool,
    pub providers: Vec<ProviderStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub v: u32,
    pub name: String,
    pub platform: String,
    /// `host:port` candidates, tried in order. LAN and Tailscale addresses both work.
    pub addrs: Vec<String>,
    /// SHA-256 of the sharing device's certificate (hex).
    pub fp: String,
    /// One-time secret (hex, 32 bytes). Consumed on first use.
    pub secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InviteError {
    #[error("This is not an Aillies pairing code.")]
    Format,
    #[error("This pairing code is from an unsupported Aillies version.")]
    Version,
    #[error("This pairing code is damaged.")]
    Invalid,
}

pub fn is_fingerprint(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub fn valid_name(s: &str) -> bool {
    let n = s.chars().count();
    (1..=64).contains(&n) && !s.chars().any(char::is_control)
}

impl Invite {
    pub fn validate(&self) -> Result<(), InviteError> {
        if self.v != PROTOCOL_VERSION {
            return Err(InviteError::Version);
        }
        let ok = valid_name(&self.name)
            && self.platform.len() <= 16
            && is_fingerprint(&self.fp)
            && self.secret.len() == 64
            && self.secret.bytes().all(|b| b.is_ascii_hexdigit())
            && !self.addrs.is_empty()
            && self.addrs.len() <= 16
            && self.addrs.iter().all(|a| a.len() <= 260 && a.contains(':') && !a.chars().any(char::is_whitespace));
        if ok { Ok(()) } else { Err(InviteError::Invalid) }
    }

    pub fn encode(&self) -> String {
        format!("{INVITE_PREFIX}{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).unwrap_or_default()))
    }

    pub fn decode(code: &str) -> Result<Self, InviteError> {
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        let raw = code.strip_prefix(INVITE_PREFIX).ok_or(InviteError::Format)?;
        if raw.len() > 8192 {
            return Err(InviteError::Invalid);
        }
        let bytes = URL_SAFE_NO_PAD.decode(raw).map_err(|_| InviteError::Invalid)?;
        let invite: Invite = serde_json::from_slice(&bytes).map_err(|_| InviteError::Invalid)?;
        invite.validate()?;
        Ok(invite)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairRequest {
    pub protocol: u32,
    pub secret: String,
    pub name: String,
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum PairState {
    /// Waiting for a person on the sharing device to compare codes and approve.
    Pending { id: String, code: String },
    Approved,
    Rejected,
    Expired,
}

const WORDS: [&str; 64] = [
    "amber", "arch", "bay", "birch", "blue", "brook", "cedar", "cliff", "cloud", "coast", "coral", "crane", "dawn", "delta",
    "dune", "ember", "fern", "field", "flint", "fox", "frost", "glade", "gold", "grove", "harbor", "hawk", "hill", "iris",
    "jade", "kite", "lake", "leaf", "lime", "lynx", "maple", "marsh", "mesa", "mint", "moon", "moss", "north", "oak",
    "ocean", "olive", "orbit", "palm", "peak", "pine", "plum", "quartz", "rain", "reef", "ridge", "river", "sage", "sand",
    "shore", "sky", "snow", "solar", "stone", "tide", "vale", "wave",
];

/// Short code both people compare. Binds both certificates and the invite
/// secret, so a different device on either side yields a different code.
/// It is a human aid only: approval also records both full fingerprints.
pub fn verification_code(host_fp: &str, client_fp: &str, secret: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"aillies-sas-v1\0");
    h.update(host_fp.as_bytes());
    h.update(b"\0");
    h.update(client_fp.as_bytes());
    h.update(b"\0");
    h.update(secret.as_bytes());
    let d = h.finalize();
    let n = u16::from_be_bytes([d[1], d[2]]) % 90 + 10;
    format!("{} · {} · {}", WORDS[(d[0] % 64) as usize].to_uppercase(), n, WORDS[(d[3] % 64) as usize].to_uppercase())
}

/// `7F3A·91C2` style short id for display next to a device name.
pub fn short_id(fp: &str) -> String {
    let up = fp.to_uppercase();
    if up.len() < 8 {
        return up;
    }
    format!("{}·{}", &up[..4], &up[4..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Invite {
        Invite {
            v: 1,
            name: "Dailyngocss".into(),
            platform: "macos".into(),
            addrs: vec!["192.168.1.20:18544".into(), "100.64.0.3:18544".into()],
            fp: "a".repeat(64),
            secret: "b".repeat(64),
        }
    }

    #[test]
    fn invite_round_trip_and_whitespace_tolerant() {
        let code = sample().encode();
        let spaced = format!("  {}\n{} ", &code[..20], &code[20..]);
        assert_eq!(Invite::decode(&spaced).unwrap(), sample());
    }

    #[test]
    fn invite_rejects_tampering() {
        assert_eq!(Invite::decode("hello"), Err(InviteError::Format));
        let mut bad = sample();
        bad.fp = "zz".into();
        assert_eq!(Invite::decode(&bad.encode()), Err(InviteError::Invalid));
        let mut v2 = sample();
        v2.v = 2;
        assert_eq!(Invite::decode(&v2.encode()), Err(InviteError::Version));
        let mut noaddr = sample();
        noaddr.addrs.clear();
        assert!(Invite::decode(&noaddr.encode()).is_err());
    }

    #[test]
    fn verification_code_binds_both_sides() {
        let a = verification_code(&"1".repeat(64), &"2".repeat(64), "s");
        assert_eq!(a, verification_code(&"1".repeat(64), &"2".repeat(64), "s"));
        assert_ne!(a, verification_code(&"1".repeat(64), &"3".repeat(64), "s"));
        assert_ne!(a, verification_code(&"1".repeat(64), &"2".repeat(64), "t"));
        assert_eq!(a.split(" · ").count(), 3);
    }

    #[test]
    fn short_ids() {
        assert_eq!(short_id(&"7f3a91c2".repeat(8)), "7F3A·91C2");
    }
}
