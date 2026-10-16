//! Network mobility does not change identities, grants, destinations or turn epochs.
//! Native Iroh monitoring heals paths. This observer adds wake hints and pinned
//! status probes only; it never resends an inference request.
use super::{Node, PeerHealth};
use crate::store::{now, Destination};
use iroh::Watcher;
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPhase {
    Checking,
    Reconnecting,
    Ready,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConnection {
    pub fp: String,
    pub phase: ConnectionPhase,
    pub health: Option<PeerHealth>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub generation: u64,
    pub peers: Vec<PeerConnection>,
}
impl NetworkStatus {
    fn select(&mut self, targets: &[String]) -> bool {
        if self.peers.iter().map(|p| &p.fp).eq(targets.iter()) {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.peers = targets.iter().map(|fp| PeerConnection { fp: fp.clone(), phase: ConnectionPhase::Checking, health: None }).collect();
        true
    }
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        for peer in &mut self.peers {
            peer.phase = ConnectionPhase::Reconnecting;
            peer.health = None;
        }
    }
    fn complete(&mut self, generation: u64, fp: &str, health: PeerHealth, elapsed: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        let Some(peer) = self.peers.iter_mut().find(|p| p.fp == fp) else {
            return false;
        };
        peer.phase = if health.reachable {
            ConnectionPhase::Ready
        } else if elapsed < 45 {
            ConnectionPhase::Reconnecting
        } else {
            ConnectionPhase::Offline
        };
        peer.health = Some(health);
        true
    }
}

/// Closed vocabulary: never stores interface names, addresses, SSIDs or locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NetworkCode {
    NetworkChanged,
    WakeDetected,
    MonitorUnavailable,
    PeerRecovered,
    PeerUnavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NetworkEvent {
    pub at_unix_seconds: u64,
    pub code: NetworkCode,
}

fn wake_gap(previous: SystemTime, current: SystemTime) -> bool {
    current.duration_since(previous).is_ok_and(|gap| gap > Duration::from_secs(20))
}

impl Node {
    /// Optional native resume/network hint. Reuses the current transport identity;
    /// no settings changes or inference replay. Native monitoring also runs automatically.
    pub async fn network_changed(self: &Arc<Self>) {
        self.refresh_network(NetworkCode::NetworkChanged).await;
    }
    fn network_event(&self, code: NetworkCode) {
        self.request_log.lock().unwrap().network(NetworkEvent { at_unix_seconds: now(), code });
    }
    async fn refresh_network(self: &Arc<Self>, code: NetworkCode) -> bool {
        let Ok(_activity) = self.update_activity() else {
            return false;
        };
        self.network.lock().unwrap().changed();
        self.network_event(code);
        // Harmless hint to the existing endpoint. Do not replace it or touch turn epochs.
        let _ = tokio::time::timeout(Duration::from_secs(2), self.internet.network_change()).await;
        true
    }
    fn network_targets(&self) -> Vec<String> {
        let s = self.snapshot();
        let mut targets = Vec::new();
        for destination in [&s.destination, &s.claude.destination] {
            if let Destination::Peer(fp) = destination {
                if s.peer(fp).is_some() && !targets.contains(fp) {
                    targets.push(fp.clone());
                }
            }
        }
        targets
    }
}

pub(super) async fn observe(node: Arc<Node>) {
    // Synthetic nodes must never subscribe to the host network or public relays.
    if node.cfg.advertise_loopback {
        return;
    }
    let monitor = netwatch::netmon::Monitor::new().await.ok();
    if monitor.is_none() {
        node.network_event(NetworkCode::MonitorUnavailable);
    }
    let mut interfaces = monitor.as_ref().map(|m| m.interface_state());
    let mut previous = interfaces.as_mut().map(|w| w.get());
    let mut wall = SystemTime::now();
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut probes = tokio::task::JoinSet::new();
    let mut next_probe = tokio::time::Instant::now();
    let mut recovery_started = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let current_wall = SystemTime::now();
                let wake = wake_gap(wall, current_wall);
                wall = current_wall;
                let current = interfaces.as_mut().map(|w| w.get());
                let changed = current.as_ref().zip(previous.as_ref()).is_some_and(|(new, old)| new.is_major_change(old));
                previous = current;
                let selected = node.network.lock().unwrap().select(&node.network_targets());
                if (wake || changed) && node.refresh_network(if wake { NetworkCode::WakeDetected } else { NetworkCode::NetworkChanged }).await || selected {
                    // Cancel status checks only. An active generation remains governed by its original retry budget.
                    probes.abort_all();
                    next_probe = tokio::time::Instant::now();
                    recovery_started = tokio::time::Instant::now();
                }
                if probes.is_empty() && tokio::time::Instant::now() >= next_probe {
                    next_probe = tokio::time::Instant::now() + Duration::from_secs(15);
                    let state = node.network.lock().unwrap().clone();
                    for peer in state.peers {
                        let node = node.clone();
                        let generation = state.generation;
                        probes.spawn(async move {
                            let Ok(_activity) = node.update_activity() else { return; };
                            let health = tokio::time::timeout(Duration::from_secs(20), node.peer_health(&peer.fp)).await.unwrap_or(PeerHealth {
                                reachable: false, latency_ms: None, addr: None, status: None, failure: Some(aillies_core::FailureCode::PeerUnavailable)
                            });
                            let mut state = node.network.lock().unwrap();
                            let before = state.peers.iter().find(|p| p.fp == peer.fp).map(|p| p.phase);
                            if state.complete(generation, &peer.fp, health, recovery_started.elapsed().as_secs()) {
                                let after = state.peers.iter().find(|p| p.fp == peer.fp).map(|p| p.phase);
                                drop(state);
                                if before != after {
                                    if after == Some(ConnectionPhase::Ready) { node.network_event(NetworkCode::PeerRecovered); }
                                    else if after == Some(ConnectionPhase::Offline) { node.network_event(NetworkCode::PeerUnavailable); }
                                }
                            }
                        });
                    }
                }
            }
            _ = probes.join_next(), if !probes.is_empty() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn health(ready: bool) -> PeerHealth {
        PeerHealth { reachable: ready, latency_ms: None, addr: None, status: None, failure: None }
    }
    #[test]
    fn recovery_discards_old_probes_and_preserves_selected_identity() {
        let mut state = NetworkStatus::default();
        state.select(&["approved-a".into(), "approved-b".into()]);
        let old = state.generation;
        state.changed();
        assert!(!state.complete(old, "approved-a", health(true), 0));
        assert_eq!(state.peers[0].phase, ConnectionPhase::Reconnecting);
        let current = state.generation;
        assert!(state.complete(current, "approved-a", health(true), 0));
        assert_eq!(state.peers[0].phase, ConnectionPhase::Ready);
        assert_eq!(state.peers[1].fp, "approved-b");
        assert_eq!(state.peers[1].phase, ConnectionPhase::Reconnecting);
        state.select(&["approved-b".into()]);
        assert!(!state.complete(current, "approved-a", health(true), 0));
        assert_eq!(state.peers.len(), 1);
    }
    #[test]
    fn unavailable_is_bounded_and_can_recover_later() {
        let mut state = NetworkStatus::default();
        state.select(&["approved".into()]);
        let g = state.generation;
        state.complete(g, "approved", health(false), 0);
        assert_eq!(state.peers[0].phase, ConnectionPhase::Reconnecting);
        state.complete(g, "approved", health(false), 46);
        assert_eq!(state.peers[0].phase, ConnectionPhase::Offline);
        state.complete(g, "approved", health(true), 60);
        assert_eq!(state.peers[0].phase, ConnectionPhase::Ready);
        assert!(state.peers[0].health.as_ref().unwrap().reachable);
    }
    #[test]
    fn sleep_gap_uses_wall_clock_and_ignores_backwards_clock() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        assert!(wake_gap(start, start + Duration::from_secs(40)));
        assert!(!wake_gap(start, start + Duration::from_secs(2)));
        assert!(!wake_gap(start, start - Duration::from_secs(40)));
    }
    #[tokio::test]
    async fn network_hint_preserves_identity_configuration_and_update_drain() {
        let dir = tempfile::tempdir().unwrap();
        let node = Node::start(crate::NodeConfig {
            home: dir.path().join("node"),
            codex_home: Some(dir.path().join("codex")),
            codex_auth_path: Some(dir.path().join("missing-auth.json")),
            codex_upstream: Some("http://127.0.0.1:1/".into()),
            claude_home: Some(dir.path().join("claude")),
            advertise_loopback: true,
            loopback_port: 0,
            peer_port: 0,
            ..crate::NodeConfig::from_env()
        })
        .await
        .unwrap();
        let id = node.internet.node_id();
        let fp = node.fingerprint().to_string();
        let state = std::fs::read(node.cfg.home.join("state.json")).ok();
        let pause = node.pause_for_update().unwrap();
        assert!(!node.refresh_network(NetworkCode::NetworkChanged).await);
        assert!(pause.idle());
        drop(pause);
        assert!(node.refresh_network(NetworkCode::NetworkChanged).await);
        assert_eq!(node.internet.node_id(), id);
        assert_eq!(node.fingerprint(), fp);
        assert_eq!(std::fs::read(node.cfg.home.join("state.json")).ok(), state);
        assert_eq!(node.status().network.generation, 1);
        node.shutdown();
    }
}
