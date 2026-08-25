//! Per-run runtime state, plus the run's configuration and result types.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use crate::core::config::Registry;
use crate::core::ports::{AgentTransport, EventSink};
use crate::core::session::SessionHandle;
use crate::core::task::TaskRegistry;

/// Everything the Lua environment needs, shared per run (single-threaded).
pub(crate) struct RuntimeState {
    pub(crate) registry: Registry,
    pub(crate) sink: Arc<dyn EventSink>,
    /// Where agent sessions come from (the ACP stdio transport by
    /// default; see [`default_transport`]).
    pub(crate) transport: Arc<dyn AgentTransport>,
    pub(crate) invocation_dir: PathBuf,
    pub(crate) tasks: Rc<TaskRegistry>,
    pub(crate) sessions: RefCell<Vec<SessionHandle>>,
    pub(crate) exit_code: Cell<Option<i32>>,
}

/// Configuration for one `ponos run`.
pub struct RunConfig {
    pub script_path: PathBuf,
    pub invocation_dir: PathBuf,
    pub registry: Registry,
    /// The output sink: `Arc<Renderer>` coerces here at construction
    /// sites, so callers keep building the terminal renderer.
    pub renderer: Arc<dyn EventSink>,
}

/// Result of one run.
#[derive(Debug, Default)]
pub struct RunOutcome {
    pub code: i32,
    /// Uncaught script error (printed to stderr by the CLI).
    pub error: Option<String>,
    /// Task errors never delivered to the script (printed to stderr; run fails).
    pub undelivered_errors: Vec<String>,
}

pub(crate) fn runtime_state(lua: &mlua::Lua) -> mlua::Result<Rc<RuntimeState>> {
    lua.app_data_ref::<Rc<RuntimeState>>()
        .map(|s| Rc::clone(&s))
        .ok_or_else(|| mlua::Error::runtime("ponos runtime state missing"))
}

/// The transport the pinned entry points (`run`, `setup_lua`) compose by
/// default: the ACP stdio adapter. This one line is the composition the
/// workspace split (change ②) moves into `cli` — `RunConfig` gains an
/// injected transport there, where the test surface may be updated with
/// it. Everything else in this module tree is typed against the port.
pub(crate) fn default_transport() -> Arc<dyn AgentTransport> {
    Arc::new(crate::acp::Transport)
}
