//! Codex provider. Only the sharing device runs the executor; only it reads
//! its own Codex login file. Nothing here logs prompts, tokens or output.

pub mod auth;
pub mod config;
pub mod executor;

pub use auth::{codex_home, default_auth_path, CodexOAuth};
pub use executor::{Executor, TurnStream};
