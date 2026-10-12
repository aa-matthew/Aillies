//! Aillies node: one process per device. It can send through a paired device
//! and share its own account at the same time.
//!
//! - Loopback API on 127.0.0.1 for Codex (`/v1/responses`) and for the CLI /
//!   desktop shell (`/control/*`), each with its own local secret.
//! - Peer listener (only while sharing) speaking pinned mutual TLS.
//!
//! Invariants: never fall back to another account or device, keep retries bounded
//! on the same route, keep OAuth material on its owning device, log no content.

pub mod body;
pub mod control;
pub mod diagnostics;
mod discovery;
pub mod identity;
mod image_mcp;
pub mod loopback;
pub mod node;
mod pairing;
pub use discovery::PairingCandidate;
pub mod peer_client;
pub mod peer_server;
mod retry;
pub mod store;
pub mod tls;

pub use node::{Node, NodeConfig, NodeStatus};

mod claude_route;
