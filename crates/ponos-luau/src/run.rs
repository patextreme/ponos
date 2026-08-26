//! The run entrypoint and end-of-run semantics: drive the script to
//! completion (or failure), wait for outstanding tasks, tear down agent
//! sessions, and report the outcome.

use std::rc::Rc;
use std::time::Duration;

use ponos_core::error::ExitSignal;
use ponos_core::session::SessionHandle;
use ponos_core::task;

use super::sandbox::setup_lua;
use super::state::{RunConfig, RunOutcome, RuntimeState};

// ---------------------------------------------------------------------------
// Run loop
// ---------------------------------------------------------------------------

/// Terminate and reap every agent subprocess; optionally cancel in-flight
/// turns first (error / explicit-exit paths).
async fn teardown(state: &Rc<RuntimeState>, cancel: bool) {
    let sessions: Vec<SessionHandle> = state.sessions.borrow().iter().cloned().collect();
    for s in &sessions {
        if cancel {
            s.cancel();
        }
        s.close();
    }
    for s in sessions {
        s.join().await;
    }
    state.sessions.borrow_mut().clear();
}

/// Wait for all outstanding spawned tasks to complete (script-end drain).
async fn wait_outstanding(state: &Rc<RuntimeState>) {
    while !state.tasks.pending().is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Run one script to completion (or failure) and report the outcome.
/// Must be called inside a `LocalSet` (task futures are `!Send`).
pub async fn run(cfg: RunConfig) -> RunOutcome {
    let script_path = cfg.script_path.clone();

    let lua = match setup_lua(&cfg) {
        Ok(lua) => lua,
        Err(e) => {
            return RunOutcome {
                code: 1,
                error: Some(format!("failed to initialize script environment: {e}")),
                undelivered_errors: vec![],
            };
        }
    };

    let state: Rc<RuntimeState> = lua
        .app_data_ref::<Rc<RuntimeState>>()
        .map(|s| Rc::clone(&s))
        .expect("runtime state installed");

    let source = match std::fs::read_to_string(&script_path) {
        Ok(s) => s,
        Err(e) => {
            return RunOutcome {
                code: 1,
                error: Some(format!("cannot read script {}: {e}", script_path.display())),
                undelivered_errors: vec![],
            };
        }
    };

    let abs = std::fs::canonicalize(&script_path).unwrap_or(script_path.clone());
    let chunk = lua.load(source).set_name(format!("@{}", abs.display()));

    let result: mlua::Result<()> = chunk.eval_async().await;

    match result {
        Ok(()) => {}
        Err(e) => {
            if let Some(sig) = e.downcast_ref::<ExitSignal>() {
                // Explicit exit: pending tasks and processes torn down; the
                // exit code wins over any undelivered task errors.
                teardown(&state, true).await;
                return RunOutcome {
                    code: sig.code,
                    error: None,
                    undelivered_errors: vec![],
                };
            }
            // Uncaught script error: cancel in-flight turns, tear down, exit 1.
            teardown(&state, true).await;
            return RunOutcome {
                code: 1,
                error: Some(task::display_error(&e)),
                undelivered_errors: vec![],
            };
        }
    }

    // Normal end: wait for outstanding tasks first.
    wait_outstanding(&state).await;

    // An exit signalled from within a (now-finished) task still wins.
    if let Some(code) = state.exit_code.get() {
        teardown(&state, true).await;
        return RunOutcome {
            code,
            error: None,
            undelivered_errors: vec![],
        };
    }

    let undelivered = state.tasks.undelivered_errors();
    teardown(&state, false).await;
    RunOutcome {
        code: i32::from(!undelivered.is_empty()),
        error: None,
        undelivered_errors: undelivered,
    }
}
