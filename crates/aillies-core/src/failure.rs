//! Fixed failure categories. Never embed upstream bodies, tokens or identifiers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    // Host account (the sharing device)
    AccountNotSelected,
    InvalidAuthFile,
    OauthExpired,
    AuthFileUnavailable,
    AccountChanged,
    // Upstream
    UpstreamRejected,
    UnexpectedUpstreamFormat,
    UnexpectedJsonResponse,
    StreamFailed,
    UpstreamUnavailable,
    // Local / peer path
    RequestInvalid,
    RoutingPaused,
    NoDestination,
    PeerUnavailable,
    PeerBusy,
    SharingOff,
    NotApproved,
    ProtocolMismatch,
    DeliveryUncertain,
    Unauthorized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub code: FailureCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    error: EnvelopeBody,
}

#[derive(Serialize, Deserialize)]
struct EnvelopeBody {
    code: FailureCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upstream_status: Option<u16>,
    message: String,
    #[serde(rename = "type")]
    kind: String,
}

impl Failure {
    pub const fn new(code: FailureCode) -> Self {
        Self { code, upstream_status: None }
    }

    pub const fn upstream(status: u16) -> Self {
        Self { code: FailureCode::UpstreamRejected, upstream_status: Some(status) }
    }

    /// Whether the request could have reached the account owner's upstream.
    /// Used to tell the user if a retry might double-spend.
    pub fn may_have_been_delivered(&self) -> bool {
        matches!(self.code, FailureCode::DeliveryUncertain | FailureCode::StreamFailed)
    }

    /// HTTP status used on the loopback and peer APIs.
    pub fn http_status(&self) -> u16 {
        use FailureCode::*;
        match self.code {
            RequestInvalid => 400,
            Unauthorized | NotApproved => 401,
            UpstreamRejected => match self.upstream_status {
                Some(429) => 429,
                _ => 502,
            },
            PeerBusy => 429,
            ProtocolMismatch => 426,
            RoutingPaused | NoDestination | PeerUnavailable | SharingOff | AccountNotSelected
            | InvalidAuthFile | OauthExpired | AuthFileUnavailable | AccountChanged => 503,
            UnexpectedUpstreamFormat | UnexpectedJsonResponse | StreamFailed | UpstreamUnavailable
            | DeliveryUncertain => 502,
        }
    }

    /// Short human message. Stable wording; safe to show anywhere.
    pub fn message(&self) -> String {
        use FailureCode::*;
        match self.code {
            AccountNotSelected => "The sharing device has no Codex login selected. Turn sharing on there.".into(),
            InvalidAuthFile => "The sharing device needs a valid Codex ChatGPT login. API-key logins are unsupported.".into(),
            OauthExpired => "The sharing device's Codex login expired. Open Codex there to renew it, then retry.".into(),
            AuthFileUnavailable => "The sharing device cannot read its Codex login file.".into(),
            AccountChanged => "The Codex account changed on the sharing device. Turn sharing off and on there to approve the new account.".into(),
            UpstreamRejected => match self.upstream_status {
                Some(400) => "OpenAI rejected the request format or model (HTTP 400).".into(),
                Some(401) => "OpenAI rejected the sharing device's Codex login (HTTP 401). Renew it in Codex there.".into(),
                Some(403) => "OpenAI denied this request (HTTP 403). Check the sharing account's access.".into(),
                Some(404) => "OpenAI could not find the endpoint or model (HTTP 404).".into(),
                Some(429) => "Usage or rate limit reached on the sharing account (HTTP 429).".into(),
                Some(s) => format!("OpenAI rejected the request (HTTP {s})."),
                None => "OpenAI rejected the request.".into(),
            },
            UnexpectedUpstreamFormat => "OpenAI returned an unrecognized response; no generation stream was verified.".into(),
            UnexpectedJsonResponse => "OpenAI returned JSON without a completed generation.".into(),
            StreamFailed => "The response stream failed part-way. Nothing was resent automatically.".into(),
            UpstreamUnavailable => "The sharing device could not reach OpenAI.".into(),
            RequestInvalid => "Aillies could not accept this request shape.".into(),
            RoutingPaused => "Routing is paused in Aillies. Nothing was sent.".into(),
            NoDestination => "Choose a device in Aillies first. Nothing was sent.".into(),
            PeerUnavailable => "The selected device is unreachable. Nothing was sent.".into(),
            PeerBusy => "The selected device is busy with other requests. Nothing was sent.".into(),
            SharingOff => "Sharing is off on the selected device. Nothing was sent.".into(),
            NotApproved => "This device is not approved by the selected device. Pair again.".into(),
            ProtocolMismatch => "The two devices run incompatible Aillies versions. Update both.".into(),
            DeliveryUncertain => "The connection dropped after sending. The request may have run on the sharing account. Aillies never resends automatically.".into(),
            Unauthorized => "Missing or wrong local access key.".into(),
        }
    }

    /// OpenAI-style JSON error body so Codex can display the message.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(&Envelope {
            error: EnvelopeBody {
                code: self.code,
                upstream_status: self.upstream_status,
                message: format!("Aillies: {}", self.message()),
                kind: "aillies_error".into(),
            },
        })
        .unwrap_or_default()
    }

    /// Parse an envelope produced by `to_json`. Rejects oversized or odd input.
    pub fn from_json(data: &[u8]) -> Option<Self> {
        if data.len() > 4096 {
            return None;
        }
        let env: Envelope = serde_json::from_slice(data).ok()?;
        if let Some(s) = env.error.upstream_status {
            if !(100..=599).contains(&s) {
                return None;
            }
        }
        Some(Self { code: env.error.code, upstream_status: env.error.upstream_status })
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for Failure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_round_trips_without_leaking() {
        let f = Failure::upstream(429);
        let json = f.to_json();
        let text = String::from_utf8(json.clone()).unwrap();
        assert!(text.contains("upstream_rejected"));
        assert!(text.contains("HTTP 429"));
        assert_eq!(Failure::from_json(&json), Some(f));
        assert_eq!(Failure::upstream(429).http_status(), 429);
        assert_eq!(Failure::upstream(401).http_status(), 502);
    }

    #[test]
    fn rejects_bad_envelopes() {
        assert!(Failure::from_json(br#"{"error":{"code":"upstream_rejected","upstream_status":9999,"message":"x","type":"t"}}"#).is_none());
        assert!(Failure::from_json(&vec![b' '; 5000]).is_none());
        assert!(Failure::from_json(b"<html>").is_none());
    }

    #[test]
    fn delivery_semantics() {
        assert!(Failure::new(FailureCode::DeliveryUncertain).may_have_been_delivered());
        assert!(!Failure::new(FailureCode::PeerUnavailable).may_have_been_delivered());
    }
}
