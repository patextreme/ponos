//! Per-run runtime state, plus the run's configuration and result types.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use ponos_core::config::Registry;
use ponos_core::ports::{AgentTransport, EventSink};
use ponos_core::session::SessionHandle;
use ponos_core::task::TaskRegistry;

/// Everything the Lua environment needs, shared per run (single-threaded).
pub(crate) struct RuntimeState {
    pub(crate) registry: Registry,
    pub(crate) sink: Arc<dyn EventSink>,
    /// Where agent sessions come from (the ACP stdio transport by
    /// Injected via [`RunConfig::transport`] (the CLI composes the ACP
    /// stdio adapter there).
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
    /// Where agent sessions come from: the injected ACP stdio transport
    /// (composed in the CLI crate — the adapter choice is a composition
    /// decision, not a scripting-runtime one).
    pub transport: Arc<dyn AgentTransport>,
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
