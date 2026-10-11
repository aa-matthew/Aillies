//! Local-network discovery advertises only a public certificate fingerprint and
//! connection metadata. Passwords, invite secrets and OpenAI credentials are absent.
use aillies_core::protocol::is_fingerprint;
use anyhow::{anyhow, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::{collections::HashMap, net::SocketAddr, time::Duration};
const SERVICE: &str = "_aillies-pair._tcp.local.";
pub(crate) const MAX_CANDIDATES: usize = 16;
/// Untrusted discovery result. PAKE and both-device approval establish trust.
#[derive(Clone)]
pub struct PairingCandidate {
    pub fp: String,
    pub addrs: Vec<String>,
}
pub(crate) struct Advertisement {
    daemon: ServiceDaemon,
    service: Option<String>,
}
impl Drop for Advertisement {
    fn drop(&mut self) {
        if let Some(name) = self.service.take() {
            let _ = self.daemon.unregister(&name);
        }
        let _ = self.daemon.shutdown();
    }
}
pub(crate) fn advertise(fp: &str, port: u16) -> Result<Advertisement> {
    let daemon = ServiceDaemon::new().map_err(|_| anyhow!("Nearby pairing is unavailable. Use a full pairing code instead."))?;
    let mut guard = Advertisement { daemon, service: None };
    let id = uuid::Uuid::new_v4().simple().to_string();
    let info = ServiceInfo::new(SERVICE, &id, &format!("aillies-{id}.local."), "", port, &[("fp", fp)][..])
        .map_err(|_| anyhow!("Nearby pairing could not start. Use a full pairing code instead."))?
        .enable_addr_auto();
    guard.service = Some(info.get_fullname().to_owned());
    guard.daemon.register(info).map_err(|_| anyhow!("Nearby pairing could not start. Use a full pairing code instead."))?;
    Ok(guard)
}
pub(crate) async fn discover() -> Result<Vec<PairingCandidate>> {
    let guard = Advertisement {
        daemon: ServiceDaemon::new().map_err(|_| anyhow!("Nearby discovery is unavailable. Use a full pairing code instead."))?,
        service: None,
    };
    let receiver = guard.daemon.browse(SERVICE).map_err(|_| anyhow!("Nearby discovery could not start. Use a full pairing code instead."))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut found = HashMap::new();
    while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, receiver.recv_async()).await {
        if let ServiceEvent::ServiceResolved(info) = event {
            let Some(fp) = info.get_property_val_str("fp").filter(|fp| is_fingerprint(fp)) else { continue };
            if info.get_port() == 0 {
                continue;
            }
            let mut addrs: Vec<_> = info
                .get_addresses()
                .iter()
                .filter(|ip| !ip.is_unspecified() && !ip.is_multicast())
                .map(|ip| SocketAddr::new(*ip, info.get_port()).to_string())
                .collect();
            addrs.sort();
            addrs.truncate(16);
            if !addrs.is_empty() && (found.len() < MAX_CANDIDATES || found.contains_key(fp)) {
                found.insert(fp.to_owned(), PairingCandidate { fp: fp.into(), addrs });
            }
        }
    }
    let _ = guard.daemon.stop_browse(SERVICE);
    Ok(found.into_values().collect())
}
