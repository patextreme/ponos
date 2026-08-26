//! ACP client wiring: agent process spawning, the `initialize` handshake,
//! the per-session driver, run-end teardown, and typed-result injection.
//!
//! Each ponos session owns one agent subprocess. A driver task runs the
//! JSON-RPC connection: it performs the handshake, creates the ACP session,
//! then serves a command channel (`prompt` / `set_config` / `cancel` /
//! `close`). Streaming `session/update` notifications are folded into the
//! in-flight turn's accumulator and emitted as structured events through
//! the sink. ponos declares exactly one client capability — the
//! non-interactive `session.configOptions` — so capability-gating agents
//! may offer per-session config options; agent-to-client requests are
//! still answered automatically so turns never hang: fs/terminal/
//! elicitation (and anything else unknown) with a JSON-RPC "method not
//! found" (-32601) error by the dispatch chain, and
//! `session/request_permission` by the interaction policy (headless:
//! prefer `AllowAlways`, else the first other allow option) registered
//! below it.
//!
//! Sessions with a typed result contract (`agent:session({ resultSchema = … })`)
//! additionally bind a per-session Unix-domain result channel and offer
//! the agent the `ponos __bridge` MCP server in `session/new`; accepted
//! submissions land in the in-flight turn's slot and ride out on
//! `TurnOutcome::result`.
//!
//! Interior layout: [`driver`] (command loop, turn driving, event
//! emission, the [`AgentTransport`] impl), [`process`] (spawn, stderr
//! pump, kill/reap), [`proto`] (typed requests, handshake, capability
//! negotiation). The session façade types live in `core::session` and are
//! re-exported here for the pinned public API.

mod driver;
mod process;
mod proto;

pub use driver::{Transport, start_session};

pub use ponos_core::session::{
    SessionError, SessionHandle, SessionOptions, TurnError, TurnOutcome, UsageCounts,
};
