//! ponos: Luau-scripted multi-agent orchestration over the Agent Client Protocol.

pub mod acp;
pub mod bridge;
pub mod check;
pub mod cli;
pub mod config;
pub mod core;
pub mod render;
pub mod result_contract;
pub mod script;

/// Compat re-export: task semantics live in `core::task`.
pub use crate::core::task;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
