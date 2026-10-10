//! Device identity: a self-signed P-256 certificate pinned by SHA-256.

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Clone)]
pub struct Identity {
    pub cert: CertificateDer<'static>,
    key: Vec<u8>,
    pub fingerprint: String,
}

pub fn fingerprint(der: &[u8]) -> String {
    hex::encode(Sha256::digest(der))
}

impl Identity {
    pub fn key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.clone()))
    }

    pub fn load_or_create(dir: &Path) -> Result<Self> {
        let cert_path = dir.join("identity.cert.der");
        let key_path = dir.join("identity.key.der");
        if cert_path.exists() && key_path.exists() {
            let cert = std::fs::read(&cert_path).context("read identity certificate")?;
            let key = std::fs::read(&key_path).context("read identity key")?;
            return Ok(Self { fingerprint: fingerprint(&cert), cert: CertificateDer::from(cert), key });
        }
        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
        let mut params = rcgen::CertificateParams::new(vec!["aillies.device".to_string()])?;
        params.distinguished_name.push(rcgen::DnType::CommonName, "Aillies device");
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2124, 1, 1);
        let cert = params.self_signed(&key_pair)?;
        let cert_der = cert.der().to_vec();
        let key_der = key_pair.serialize_der();
        crate::store::private_write(&key_path, &key_der)?;
        crate::store::private_write(&cert_path, &cert_der)?;
        Ok(Self { fingerprint: fingerprint(&cert_der), cert: CertificateDer::from(cert_der), key: key_der })
    }
}
