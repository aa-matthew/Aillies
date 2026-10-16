//! In-memory update barrier. Never changes routing, client settings or saved state.
use crate::body::Body;
use crate::Node;
use anyhow::{anyhow, Result};
use http_body_util::BodyExt;
use hyper::Response;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(crate) struct ActivityGate(Mutex<(bool, usize)>);
pub struct UpdateActivity(Arc<Node>);
pub struct UpdatePause(Arc<Node>);

impl Node {
    /// Cover an entire operation, including response-body consumption.
    pub fn update_activity(self: &Arc<Self>) -> Result<UpdateActivity> {
        let mut gate = self.update_gate.0.lock().unwrap();
        if gate.0 {
            return Err(anyhow!("Aillies is preparing an update. Try again after it restarts."));
        }
        gate.1 += 1;
        Ok(UpdateActivity(self.clone()))
    }
    pub fn pause_for_update(self: &Arc<Self>) -> Result<UpdatePause> {
        let _change = self.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish."))?;
        if self.activation.lock().unwrap().is_some() {
            return Err(anyhow!("Finish or cancel the provider switch before updating."));
        }
        let mut gate = self.update_gate.0.lock().unwrap();
        if gate.0 {
            return Err(anyhow!("An update is already being prepared."));
        }
        gate.0 = true;
        Ok(UpdatePause(self.clone()))
    }
}
impl UpdatePause {
    pub fn idle(&self) -> bool {
        self.0.update_gate.0.lock().unwrap().1 == 0
    }
    /// The caller holds the pause through installation/restart. Existing config
    /// transactions must finish; new HTTP/desktop operations cannot enter.
    pub fn install<T>(&self, install: impl FnOnce() -> Result<T>) -> Result<T> {
        let _change = self.0.codex_change.try_lock().map_err(|_| anyhow!("Wait for the provider change to finish."))?;
        let _connection = self.0.claude_connection_change.try_lock().map_err(|_| anyhow!("Wait for the account connection to finish."))?;
        if !self.idle() || self.0.activation.lock().unwrap().is_some() {
            return Err(anyhow!("Wait for active work to finish before updating."));
        }
        install()
    }
}
impl Drop for UpdatePause {
    fn drop(&mut self) {
        self.0.update_gate.0.lock().unwrap().0 = false;
    }
}
impl Drop for UpdateActivity {
    fn drop(&mut self) {
        self.0.update_gate.0.lock().unwrap().1 -= 1;
    }
}
pub(crate) fn retain(response: Response<Body>, activity: UpdateActivity) -> Response<Body> {
    response.map(|body| {
        body.map_frame(move |frame| {
            let _keep = &activity;
            frame
        })
        .boxed_unsync()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeConfig;
    #[tokio::test]
    async fn update_waits_for_response_drop_blocks_new_work_and_resumes_on_failure() {
        let home = tempfile::tempdir().unwrap();
        let node = Node::start(NodeConfig {
            home: home.path().join("node"),
            codex_home: Some(home.path().join("codex")),
            claude_home: Some(home.path().join("claude")),
            loopback_port: 0,
            peer_port: 0,
            advertise_loopback: true,
            ..NodeConfig::from_env()
        })
        .await
        .unwrap();
        let state = std::fs::read(node.cfg.home.join("state.json")).ok();
        let response = retain(crate::body::json(200, &serde_json::json!({})), node.update_activity().unwrap());
        let pause = node.pause_for_update().unwrap();
        assert!(!pause.idle());
        assert!(node.update_activity().is_err());
        assert!(node.pause_for_update().is_err());
        assert!(pause.install(|| Ok(())).is_err());
        drop(response);
        assert!(pause.idle());
        assert!(pause.install::<()>(|| Err(anyhow!("synthetic install refusal"))).is_err());
        drop(pause);
        assert!(node.update_activity().is_ok());
        assert_eq!(std::fs::read(node.cfg.home.join("state.json")).ok(), state);
        node.shutdown();
    }
}
