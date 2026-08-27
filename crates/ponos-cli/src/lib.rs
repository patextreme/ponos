//! ponos: Luau-scripted multi-agent orchestration over the Agent Client
//! Protocol.
//!
//! This crate is the composition root *and* the permanent `ponos` facade:
//! a flat re-export of the workspace member crates, so existing imports
//! (`ponos::acp`, `ponos::script`, `ponos::render`, `ponos::config`,
//! `ponos::task`, …) keep resolving unchanged. Adapter selection (the
//! ACP stdio transport behind `AgentTransport`) is composed here in
//! `cli`, the only crate allowed to see every member.
//!
//! Facade rules (change ② design D3): flat `pub use` list, no glob
//! re-exports, no logic. The member crates are private workspace
//! members; this surface is the package's public API.

pub use ponos_acp as acp;
pub use ponos_check as check;
pub use ponos_config as config_fs;
pub use ponos_luau as script;
pub use ponos_render as render;
pub use ponos_result as result_wire;

/// Compat re-export: the config model lives in `ponos-core`.
pub use ponos_core::config;

/// Compat re-export: task semantics live in `ponos-core`.
pub use ponos_core::task;

/// Compat re-export: the version string lives in `ponos-core`.
pub use ponos_core::VERSION;

pub mod bridge;
pub mod cli;
pub mod exec;
