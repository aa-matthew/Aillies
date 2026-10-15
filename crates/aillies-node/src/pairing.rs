//! Eight-digit password bootstrap. Neither the password nor an OpenAI token goes
//! on the wire. SPAKE2 binds both TLS certificate identities; AES-GCM authenticates
//! the full existing invite before ordinary pinned-TLS pairing and human approval.
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AeadRng, Payload},
    Aes256Gcm, Nonce,
};
use aillies_core::protocol::{is_fingerprint, pairing_digits, Invite, INVITE_TTL_SECS};
use aillies_core::{Failure, FailureCode};
use rand::{rngs::OsRng, Rng};
use serde::{Deserialize, Serialize};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
const MAX_GUESSES: usize = 5;
const MAX_SEALED_BYTES: usize = 16 * 1024;
fn refused() -> Failure {
    Failure::new(FailureCode::NotApproved)
}
fn identities(client: &str, server: &str) -> (Identity, Identity) {
    (Identity::new(format!("aillies-numeric-client-v1\0{client}").as_bytes()), Identity::new(format!("aillies-numeric-server-v1\0{server}").as_bytes()))
}
fn aad(client: &str, server: &str) -> Vec<u8> {
    format!("aillies-sealed-invite-v1\0{client}\0{server}").into_bytes()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BootstrapRequest {
    pub message: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BootstrapReply {
    pub message: String,
    pub nonce: String,
    pub sealed: String,
}

pub(crate) struct NumericInvite {
    pub(crate) invite: Invite,
    pub(crate) permissions: Option<crate::node::PairPermissions>,
    pub(crate) generation: uuid::Uuid,
    digits: String,
    expires: Instant,
    guesses: usize,
    issued_to: HashSet<String>,
    pub(crate) advertisement: Option<crate::discovery::Advertisement>,
}
impl NumericInvite {
    pub(crate) fn new(invite: Invite) -> Self {
        Self {
            invite,
            permissions: None,
            generation: uuid::Uuid::new_v4(),
            digits: format!("{:08}", OsRng.gen_range(0..100_000_000_u32)),
            expires: Instant::now() + Duration::from_secs(INVITE_TTL_SECS),
            guesses: 0,
            issued_to: HashSet::new(),
            advertisement: None,
        }
    }
    pub(crate) fn code(&self) -> &str {
        &self.digits
    }
    pub(crate) fn expired(&self) -> bool {
        Instant::now() >= self.expires
    }
    pub(crate) fn accepts_secret(&self, client_fp: &str, secret: &str) -> bool {
        use subtle::ConstantTimeEq;
        !self.expired()
            && self.issued_to.contains(client_fp)
            && secret.len() == self.invite.secret.len()
            && bool::from(secret.as_bytes().ct_eq(self.invite.secret.as_bytes()))
    }
    pub(crate) fn bootstrap(&mut self, client_fp: &str, request: &BootstrapRequest) -> Result<BootstrapReply, Failure> {
        if self.expired() || self.guesses >= MAX_GUESSES || !is_fingerprint(client_fp) {
            return Err(refused());
        }
        // Every active PAKE execution permits at most one online password guess.
        self.guesses += 1;
        if self.guesses == MAX_GUESSES {
            self.advertisement.take();
        }
        if request.message.len() != 66 {
            return Err(refused());
        }
        let message = hex::decode(&request.message).map_err(|_| refused())?;
        let (a, b) = identities(client_fp, &self.invite.fp);
        let (server, out) = Spake2::<Ed25519Group>::start_b(&Password::new(self.digits.as_bytes()), &a, &b);
        let key = server.finish(&message).map_err(|_| refused())?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| refused())?;
        let nonce = Aes256Gcm::generate_nonce(&mut AeadRng);
        let plain = serde_json::to_vec(&self.invite).map_err(|_| refused())?;
        let sealed = cipher.encrypt(&nonce, Payload { msg: &plain, aad: &aad(client_fp, &self.invite.fp) }).map_err(|_| refused())?;
        self.issued_to.insert(client_fp.into());
        Ok(BootstrapReply { message: hex::encode(out), nonce: hex::encode(nonce), sealed: hex::encode(sealed) })
    }
}
pub(crate) struct ClientHandshake {
    state: Spake2<Ed25519Group>,
    client_fp: String,
    server_fp: String,
}
impl ClientHandshake {
    pub(crate) fn start(code: &str, client_fp: &str, server_fp: &str) -> Result<(Self, BootstrapRequest), Failure> {
        let digits = pairing_digits(code).ok_or_else(refused)?;
        if !is_fingerprint(client_fp) || !is_fingerprint(server_fp) {
            return Err(refused());
        }
        let (a, b) = identities(client_fp, server_fp);
        let (state, message) = Spake2::<Ed25519Group>::start_a(&Password::new(digits.as_bytes()), &a, &b);
        Ok((Self { state, client_fp: client_fp.into(), server_fp: server_fp.into() }, BootstrapRequest { message: hex::encode(message) }))
    }
    pub(crate) fn finish(self, response: BootstrapReply) -> Result<Invite, Failure> {
        if response.message.len() != 66 || response.nonce.len() != 24 || response.sealed.len() > MAX_SEALED_BYTES * 2 {
            return Err(refused());
        }
        let message = hex::decode(response.message).map_err(|_| refused())?;
        let key = self.state.finish(&message).map_err(|_| refused())?;
        let nonce = hex::decode(response.nonce).map_err(|_| refused())?;
        let sealed = hex::decode(response.sealed).map_err(|_| refused())?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| refused())?;
        let bytes = cipher.decrypt(Nonce::from_slice(&nonce), Payload { msg: &sealed, aad: &aad(&self.client_fp, &self.server_fp) }).map_err(|_| refused())?;
        let invite: Invite = serde_json::from_slice(&bytes).map_err(|_| refused())?;
        invite.validate().map_err(|_| refused())?;
        if invite.fp != self.server_fp {
            return Err(refused());
        }
        Ok(invite)
    }
}
#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
