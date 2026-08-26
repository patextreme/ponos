//! The Luau scripting environment: sandbox, curated stdlib, custom require,
//! the `ponos` namespace bindings, and the run loop with end-of-run
//! semantics (wait for outstanding tasks, teardown agents, exit codes).
//!
//! Interior layout: [`state`] (the per-run runtime state and the run's
//! config/result types), [`sandbox`] (the sandboxed environment setup),
//! [`bindings`] (the `ponos.*` namespace and object constructors),
//! [`run`] (the entrypoint with end-of-run semantics), and [`require`]
//! (relative module resolution).

pub mod require;

mod bindings;
mod run;
mod sandbox;
mod state;

pub use run::run;
pub use sandbox::setup_lua;
pub use state::{RunConfig, RunOutcome};
