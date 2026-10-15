//! Iroh carries the existing pinned mTLS stream; it never authorizes a peer.
use crate::{node::Node, store::private_write};
use anyhow::{ensure, Result};
use iroh::{Endpoint, NodeAddr, NodeId, RelayMap, RelayMode, SecretKey, Watcher};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncWriteExt, DuplexStream},
    sync::Semaphore,
};

const ALPN: &[u8] = b"aillies/pinned-tls/1";

const PRODUCTION_RELAYS: [&str; 3] = ["https://aps1-1.relay.n0.iroh.link/", "https://use1-1.relay.n0.iroh.link/", "https://euc1-1.relay.n0.iroh.link/"];
fn production_relays() -> Result<RelayMap> {
    let urls: std::result::Result<Vec<iroh::RelayUrl>, _> = PRODUCTION_RELAYS.iter().map(|u| u.parse()).collect();
    Ok(urls.map_err(|_| anyhow::anyhow!("Aillies relay configuration needs attention."))?.into_iter().collect())
}

pub(crate) fn is_ready(endpoint: &Endpoint) -> bool {
    !endpoint.home_relay().get().is_empty()
}

pub(crate) async fn bind(home: &Path, isolated: bool) -> Result<Endpoint> {
    let path = home.join("iroh.key");
    let key = if std::fs::symlink_metadata(&path).is_ok() {
        let meta = std::fs::symlink_metadata(&path)?;
        ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len() == 32, "Aillies internet identity needs attention.");
        let bytes: [u8; 32] = std::fs::read(&path)?.try_into().map_err(|_| anyhow::anyhow!("Aillies internet identity needs attention."))?;
        SecretKey::from_bytes(&bytes)
    } else {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        private_write(&path, &bytes)?;
        SecretKey::from_bytes(&bytes)
    };
    let builder = if isolated {
        Endpoint::builder().clear_discovery().relay_mode(RelayMode::Disabled).bind_addr_v4("127.0.0.1:0".parse()?)
    } else {
        // Older Iroh releases bundle retired relay hostnames. Use current official n0 hosts.
        Endpoint::builder().relay_mode(RelayMode::Custom(production_relays()?)).discovery_n0()
    };
    builder.secret_key(key).alpns(vec![ALPN.to_vec()]).bind().await.map_err(|_| anyhow::anyhow!("Could not start Aillies internet connection."))
}

pub(crate) fn address(endpoint: &Endpoint, isolated: bool) -> String {
    if isolated {
        if let Some(addr) = endpoint.bound_sockets().into_iter().find(|a| a.is_ipv4()) {
            return format!("iroh:{}@{addr}", endpoint.node_id());
        }
    }
    if let Some(relay) = endpoint.home_relay().get().first() {
        return format!("iroh:{}#{relay}", endpoint.node_id());
    }
    format!("iroh:{}", endpoint.node_id())
}

/// Full codes carry a ready relay hint, avoiding first-use DNS publication races.
pub(crate) async fn ready(endpoint: &Endpoint) -> bool {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !is_ready(endpoint) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .is_ok()
}

fn parse(address: &str) -> Option<NodeAddr> {
    let address = address.strip_prefix("iroh:")?;
    let (address, relay) = address.split_once('#').map_or((address, None), |(id, relay)| (id, Some(relay)));
    let (id, socket) = address.split_once('@').map_or((address, None), |(id, socket)| (id, Some(socket)));
    let id: NodeId = id.parse().ok()?;
    let mut addr = NodeAddr::new(id);
    if let Some(socket) = socket {
        addr = addr.with_direct_addresses([socket.parse().ok()?]);
    }
    if let Some(relay) = relay {
        let relay: iroh::RelayUrl = relay.parse().ok()?;
        if relay.scheme() != "https" || !relay.username().is_empty() || relay.password().is_some() || relay.fragment().is_some() {
            return None;
        }
        addr = addr.with_relay_url(relay);
    }
    Some(addr)
}

/// Bounded stream bridge. Holding Connection prevents premature QUIC closure.
fn stream(connection: iroh::endpoint::Connection, send: iroh::endpoint::SendStream, recv: iroh::endpoint::RecvStream) -> DuplexStream {
    let (client, bridge) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let (mut read, mut write) = tokio::io::split(bridge);
        let mut send = send;
        let mut recv = recv;
        let outgoing = async {
            tokio::io::copy(&mut read, &mut send).await.ok();
            send.shutdown().await.ok();
        };
        let incoming = async {
            tokio::io::copy(&mut recv, &mut write).await.ok();
            write.shutdown().await.ok();
        };
        tokio::select! {
            _ = outgoing => {}
            _ = incoming => {}
            _ = connection.closed() => {}
            _ = tokio::time::sleep(Duration::from_secs(600)) => {}
        }
        connection.close(0u32.into(), b"");
    });
    client
}

pub(crate) async fn dial(endpoint: &Endpoint, address: &str) -> Option<DuplexStream> {
    let connection = endpoint.connect(parse(address)?, ALPN).await.ok()?;
    let (send, recv) = connection.open_bi().await.ok()?;
    Some(stream(connection, send, recv))
}

pub(crate) async fn serve(node: Arc<Node>) {
    let slots = Arc::new(Semaphore::new(64));
    while let Some(incoming) = node.internet.accept().await {
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            incoming.refuse();
            continue;
        };
        if node.peer_port().is_none() {
            incoming.refuse();
            continue;
        }
        let node = node.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let mut epoch = node.epoch.subscribe();
            let accept = async {
                let Ok(Ok(connection)) = tokio::time::timeout(Duration::from_secs(10), incoming).await else { return };
                let Ok(Ok((send, recv))) = tokio::time::timeout(Duration::from_secs(10), connection.accept_bi()).await else { return };
                // Check again after the handshake, before accepting mTLS/application bytes.
                if node.peer_port().is_none() {
                    return;
                }
                let Ok(tls) = crate::tls::server_config(&node.identity) else { return };
                crate::peer_server::serve_stream(node.clone(), tokio_rustls::TlsAcceptor::from(tls), stream(connection, send, recv), &mut epoch).await;
            };
            let mut changed = node.epoch.subscribe();
            tokio::select! {
                _ = accept => {}
                _ = changed.changed() => {}
                _ = tokio::time::sleep(Duration::from_secs(600)) => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_relays_use_current_official_hosts() {
        let map = production_relays().unwrap();
        assert_eq!(map.urls().count(), 3);
        for url in map.urls() {
            assert_eq!(url.scheme(), "https");
            assert!(matches!(url.host_str(), Some("aps1-1.relay.n0.iroh.link." | "use1-1.relay.n0.iroh.link." | "euc1-1.relay.n0.iroh.link.")));
        }
    }
    #[test]
    fn rejects_malformed_addresses() {
        for value in ["https://example.com", "iroh:bad", "iroh:bad@127.0.0.1:1", "iroh:"] {
            assert!(parse(value).is_none());
        }
    }
    #[test]
    fn relay_hint_roundtrips_without_allowing_insecure_or_credential_urls() {
        let id = SecretKey::from_bytes(&[7; 32]).public();
        let addr = parse(&format!("iroh:{id}#https://relay.example/")).unwrap();
        assert_eq!(addr.node_id, id);
        assert!(addr.relay_url.is_some());
        for url in ["http://relay.example/", "https://user:password@relay.example/", "https://relay.example/#fragment"] {
            assert!(parse(&format!("iroh:{id}#{url}")).is_none());
        }
    }
    #[tokio::test]
    async fn private_identity_is_stable_and_refuses_redirected_keys() {
        let dir = tempfile::tempdir().unwrap();
        let a = bind(dir.path(), true).await.unwrap();
        let id = a.node_id();
        a.close().await;
        let b = bind(dir.path(), true).await.unwrap();
        assert_eq!(b.node_id(), id);
        assert!(parse(&address(&b, true)).is_some());
        b.close().await;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};
            let path = dir.path().join("iroh.key");
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
            std::fs::remove_file(&path).unwrap();
            symlink(dir.path().join("other"), &path).unwrap();
            assert!(bind(dir.path(), true).await.is_err());
        }
    }
}
