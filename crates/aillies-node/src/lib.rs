//! Aillies node: one process per device. It can send through a paired device
//! and share its own account at the same time.
//!
//! - Loopback API on 127.0.0.1 for Codex (`/v1/responses`) and for the CLI /
//!   desktop shell (`/control/*`), each with its own local secret.
//! - Peer listener (only while sharing) speaking pinned mutual TLS.
//!
//! Invariants: never fall back to another account or device, never replay a
//! request, keep OAuth material on the device that owns it, log no content.

pub mod body;
pub mod control;
pub mod identity;
pub mod loopback;
pub mod node;
pub mod peer_client;
pub mod peer_server;
pub mod store;
pub mod tls;

pub use node::{Node, NodeConfig, NodeStatus};
