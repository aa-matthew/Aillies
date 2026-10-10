//! Local control API used by the `aillies` CLI. The desktop shell calls the
//! same `Node` methods directly.

use crate::body::{self, Body};
use crate::node::Node;
use hyper::body::Incoming;
use hyper::{Request, Response};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[derive(Deserialize, Default)]
struct Args {
    #[serde(default)]
    on: Option<bool>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    wait_secs: Option<u64>,
}

fn ok(v: serde_json::Value) -> Response<Body> {
    body::json(200, &v)
}

fn bad(e: impl std::fmt::Display) -> Response<Body> {
    body::json(400, &json!({"error": e.to_string()}))
}

pub async fn handle(node: Arc<Node>, req: Request<Incoming>) -> Response<Body> {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().trim_start_matches("/control/").to_string();
    let raw = body::read_limited(req.into_body(), 16 * 1024).await.unwrap_or_default();
    let args: Args = if raw.is_empty() { Args::default() } else { serde_json::from_slice(&raw).unwrap_or_default() };
    let parts: Vec<&str> = path.split('/').collect();

    let result: anyhow::Result<serde_json::Value> = async {
        Ok(match (method.as_str(), parts.as_slice()) {
            ("GET", ["status"]) => serde_json::to_value(node.status())?,
            ("GET", ["peers", fp, "health"]) => serde_json::to_value(node.peer_health(fp).await)?,
            ("POST", ["invite"]) => json!({"code": node.create_invite().await?, "expires_in": aillies_core::protocol::INVITE_TTL_SECS}),
            ("POST", ["pair"]) => serde_json::to_value(node.start_pairing(args.code.as_deref().unwrap_or("")).await?)?,
            ("POST", ["pair", id, "confirm"]) => {
                let wait = Duration::from_secs(args.wait_secs.unwrap_or(120).min(300));
                serde_json::to_value(node.confirm_pairing(id, wait).await?)?
            }
            ("POST", ["pair", id, "cancel"]) => {
                node.cancel_pairing(id).await;
                json!({})
            }
            ("POST", ["approve", id]) => {
                node.approve(id, true)?;
                json!({})
            }
            ("POST", ["reject", id]) => {
                node.approve(id, false)?;
                json!({})
            }
            ("POST", ["destination"]) => serde_json::to_value(node.set_destination(args.target.as_deref().unwrap_or("local"))?)?,
            ("POST", ["routing"]) => {
                node.set_routing(args.on.unwrap_or(true))?;
                json!({})
            }
            ("POST", ["sharing"]) => {
                node.set_sharing(args.on.unwrap_or(true)).await?;
                json!({})
            }
            ("POST", ["codex", "enable"]) => {
                node.codex_enable(args.model.as_deref())?;
                json!({"config": node.codex_config().config_path()})
            }
            ("POST", ["codex", "restore"]) => {
                node.codex_restore()?;
                json!({})
            }
            ("POST", ["peers", "remove"]) => {
                node.remove_peer(args.target.as_deref().unwrap_or(""))?;
                json!({})
            }
            ("POST", ["clients", "revoke"]) => {
                node.revoke_client(args.target.as_deref().unwrap_or(""))?;
                json!({})
            }
            ("POST", ["name"]) => {
                node.set_name(args.name.as_deref().unwrap_or(""))?;
                json!({})
            }
            ("POST", ["dismiss"]) => {
                node.dismiss_failure();
                json!({})
            }
            _ => anyhow::bail!("Unknown control command."),
        })
    }
    .await;
    match result {
        Ok(v) => ok(v),
        Err(e) => bad(e),
    }
}
