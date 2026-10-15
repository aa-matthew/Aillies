//! Outbound pinned-TLS connections to a paired (or pairing) device.

use crate::identity::Identity;
use crate::tls::client_config;
use bytes::Bytes;
use http_body_util::Full;
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

pub type Sender = SendRequest<Full<Bytes>>;

pub struct Connected {
    pub sender: Sender,
    pub addr: String,
    pub latency_ms: u64,
}

const PER_ADDR_TIMEOUT: Duration = Duration::from_secs(4);

/// Try each address in order. Only a device presenting the pinned certificate
/// is accepted, so a wrong or hostile address simply fails over to the next.
/// Nothing has been sent when this returns an error.
pub async fn connect(id: &Identity, pinned_fp: &str, addrs: &[String]) -> Option<Connected> {
    connect_inner(id, None, pinned_fp, addrs).await
}

pub(crate) async fn connect_with_internet(id: &Identity, endpoint: &iroh::Endpoint, pinned_fp: &str, addrs: &[String]) -> Option<Connected> {
    connect_inner(id, Some(endpoint), pinned_fp, addrs).await
}

async fn connect_inner(id: &Identity, endpoint: Option<&iroh::Endpoint>, pinned_fp: &str, addrs: &[String]) -> Option<Connected> {
    let cfg = client_config(id, pinned_fp).ok()?;
    let connector = TlsConnector::from(cfg);
    for addr in addrs {
        let started = Instant::now();
        let attempt = async {
            if addr.starts_with("iroh:") {
                let stream = crate::internet::dial(endpoint?, addr).await?;
                return handshake(&connector, stream).await;
            }
            let tcp = TcpStream::connect(addr.as_str()).await.ok()?;
            let _ = tcp.set_nodelay(true);
            handshake(&connector, tcp).await
        };
        if let Ok(Some(sender)) = tokio::time::timeout(if addr.starts_with("iroh:") { Duration::from_secs(15) } else { PER_ADDR_TIMEOUT }, attempt).await {
            return Some(Connected { sender, addr: addr.clone(), latency_ms: started.elapsed().as_millis() as u64 });
        }
    }
    None
}

pub fn request(method: &str, path: &str, body: Bytes) -> hyper::Request<Full<Bytes>> {
    hyper::Request::builder()
        .method(method)
        .uri(path)
        .header("host", "aillies.device")
        .header("content-type", "application/json")
        .header(aillies_core::protocol::PAIR_HEADER_VERSION, aillies_core::protocol::PROTOCOL_VERSION.to_string())
        .body(Full::new(body))
        .expect("request")
}

async fn handshake<S>(connector: &TlsConnector, stream: S) -> Option<Sender>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let name = ServerName::try_from("aillies.device").ok()?;
    let tls = connector.connect(name, stream).await.ok()?;
    let (sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(tls)).await.ok()?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    Some(sender)
}
