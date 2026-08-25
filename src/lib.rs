//! ponos: Luau-scripted multi-agent orchestration over the Agent Client Protocol.

pub mod acp;
pub mod bridge;
pub mod check;
pub mod cli;
pub mod config_fs;
pub mod core;
pub mod render;
pub mod result_wire;
pub mod script;

/// Compat re-export: the config model lives in `core::config`.
pub use crate::core::config;

/// Compat re-export: task semantics live in `core::task`.
pub use crate::core::task;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
