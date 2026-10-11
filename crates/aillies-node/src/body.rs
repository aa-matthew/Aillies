//! Small helpers for hyper bodies.

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, StreamBody};
use hyper::body::Frame;
use hyper::{Response, StatusCode};

pub type Body = UnsyncBoxBody<Bytes, std::io::Error>;

pub fn full(data: impl Into<Bytes>) -> Body {
    Full::new(data.into()).map_err(|never| match never {}).boxed_unsync()
}

pub fn stream<S>(s: S) -> Body
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    StreamBody::new(s.map(|r| r.map(Frame::data))).boxed_unsync()
}

pub fn json(status: u16, value: &impl serde::Serialize) -> Response<Body> {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .body(full(serde_json::to_vec(value).unwrap_or_default()))
        .expect("response")
}

pub fn failure(f: &aillies_core::Failure) -> Response<Body> {
    Response::builder()
        .status(StatusCode::from_u16(f.http_status()).unwrap_or(StatusCode::BAD_GATEWAY))
        .header("content-type", "application/json")
        .header("cache-control", "no-store")
        .body(full(f.to_json()))
        .expect("response")
}

pub fn sse(body: Body) -> Response<Body> {
    Response::builder().status(200).header("content-type", "text/event-stream").header("cache-control", "no-store").body(body).expect("response")
}

/// Read a request body with a hard size cap.
pub async fn read_limited(body: hyper::body::Incoming, limit: usize) -> Result<Bytes, ()> {
    let mut out = Vec::new();
    let mut body = body;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| ())?;
        if let Some(d) = frame.data_ref() {
            if out.len() + d.len() > limit {
                return Err(());
            }
            out.extend_from_slice(d);
        }
    }
    Ok(Bytes::from(out))
}

/// Constant-time bearer check.
pub fn bearer_ok(headers: &hyper::HeaderMap, secret: &str) -> bool {
    use subtle::ConstantTimeEq;
    let values: Vec<_> = headers.get_all("authorization").iter().collect();
    if values.len() != 1 || secret.is_empty() {
        return false;
    }
    let expected = format!("Bearer {secret}");
    let got = values[0].as_bytes();
    got.len() == expected.len() && bool::from(got.ct_eq(expected.as_bytes()))
}
