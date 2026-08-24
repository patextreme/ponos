//! ponos: Luau-scripted multi-agent orchestration over the Agent Client Protocol.

pub mod acp;
pub mod bridge;
pub mod check;
pub mod cli;
pub mod config;
pub mod render;
pub mod result_contract;
pub mod script;
pub mod task;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
