//! Aillies core: everything that must behave identically on every platform.
//!
//! No I/O lives here. Request validation, stream framing, failure categories,
//! the peer protocol and pairing verification codes are pure functions so the
//! same fixtures can run in CI on macOS, Windows and Ubuntu.

pub mod failure;
pub mod protocol;
pub mod request;
pub mod wire;

pub use failure::{Failure, FailureCode};

/// Application version reported to peers.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Product name used in user-facing strings.
pub const PRODUCT: &str = "Aillies";
