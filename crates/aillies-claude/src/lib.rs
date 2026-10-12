//! Native Claude account provider. Credentials stay on the executing device.
pub mod auth;
pub mod config;
pub mod desktop_auth;
pub mod desktop_config;
pub mod executor;
pub mod oauth;
pub use config::Config;
pub use executor::Executor;
