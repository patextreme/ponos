//! The run entrypoint and end-of-run semantics: drive the script to
//! completion (or failure), wait for outstanding tasks, tear down agent
//! sessions and in-flight execs, and report the outcome.

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
    kill_inflight_execs(state).await;
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

/// Kill every in-flight `ponos.exec` process group: signal each
/// registered exec, which makes the awaiting binding drop its port
/// future — and dropping that future is the runner's kill-the-group
/// cancel-safety contract (so this is a bounded wait for kills that are
/// already done, not the kill mechanism itself). Zombies never outlive
/// the run: teardown waits until every exec is dead — deregistered, or
/// its coroutine already dropped (an abandoned root future took the
/// port future with it, and the kill-on-drop guard fired synchronously).
async fn kill_inflight_execs(state: &Rc<RuntimeState>) {
    for entry in state.execs.borrow().iter() {
        entry.killed.set(true);
        entry.cancel.notify_waiters();
    }
    // Yield in small steps so the LocalSet can poll the woken exec
    // coroutines: a live callback (two owners — the registry plus the
    // coroutine holding it) drops its port future and deregisters. An
    // entry with one owner (the registry alone) belonged to a coroutine
    // already dropped — outer cancellation abandons the root future,
    // and with it the port future — so it is dead by construction, not
    // a straggler worth waiting on.
    for _ in 0..400 {
        if state
            .execs
            .borrow()
            .iter()
            .all(|e| Rc::strong_count(e) == 1)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // Deadline (2s): proceed regardless — the kill fired synchronously in
    // the drop, so a straggler here is a bookkeeping lag, not a live child.
}

/// Wait for all outstanding spawned tasks to complete (script-end drain).
/// The outer-cancel watch stays live through the drain: a termination
/// signal arriving while spawned tasks park in long turns or no-budget
/// execs (a window that can stay open indefinitely after the script
/// body ends) must ride the same cancelled path as one arriving during
/// the body — otherwise the first signal is swallowed and only the
/// second signal's hard exit could end the run, skipping teardown.
/// Returns the cancel code when the watch fires before the tasks do.
async fn wait_outstanding(
    state: &Rc<RuntimeState>,
    shutdown: &mut Option<tokio::sync::watch::Receiver<i32>>,
) -> Option<i32> {
    loop {
        if state.tasks.pending().is_empty() {
            return None;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(5)) => {}
            code = shutdown_code(shutdown.as_mut()) => return Some(code),
        }
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

    // However the script ends — its own future resolving, or the
    // embedding process cancelling from outside (SIGINT/SIGTERM
    // forwarded by the composition root; the watch carries the code to
    // report, 128+signal by shell convention). Outer cancel abandons
    // the script future on the spot: root-coroutine exec port futures
    // drop with it (kill-on-drop fires synchronously), then teardown
    // signals whatever remains — spawned tasks, agent sessions — and
    // waits for the kills to land. The cancelled exec never surfaces as
    // a script error; the script's own outcome is moot.
    enum End {
        Script(mlua::Result<()>),
        Cancelled(i32),
    }
    let mut shutdown = cfg.shutdown;
    let end = tokio::select! {
        result = chunk.eval_async() => End::Script(result),
        code = shutdown_code(shutdown.as_mut()) => End::Cancelled(code),
    };
    let result = match end {
        End::Cancelled(code) => {
            teardown(&state, true).await;
            return RunOutcome {
                code,
                error: None,
                undelivered_errors: vec![],
            };
        }
        End::Script(result) => result,
    };

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

    // Normal end: wait for outstanding tasks first — still racing the
    // outer cancel, because the drain is part of the run.
    if let Some(code) = wait_outstanding(&state, &mut shutdown).await {
        teardown(&state, true).await;
        return RunOutcome {
            code,
            error: None,
            undelivered_errors: vec![],
        };
    }

    // An exit signalled from within a (now-finished) task still wins.
    if let Some(code) = state.exit_code.get() {
        teardown(&state, true).await;
        return RunOutcome {
            code,
            error: None,
            undelivered_errors: vec![],
        };
    }

    // Boundary race: the signal may have landed after the drain's last
    // watch poll. One non-blocking check keeps a first signal from
    // slipping between phases into silence. (A signal landing during a
    // teardown itself stays unpolled by design: teardown is bounded,
    // its kills run unconditionally, and the second-signal hard exit
    // remains the escape.)
    if let Some(code) = shutdown_fired(&shutdown) {
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

/// Resolve when the embedding process cancels the run; the watch value
/// is the exit code to report. `None` — and a sender dropped without
/// ever signalling — both mean "no outer cancel": never resolve.
async fn shutdown_code(shutdown: Option<&mut tokio::sync::watch::Receiver<i32>>) -> i32 {
    let Some(rx) = shutdown else {
        return std::future::pending().await;
    };
    match rx.changed().await {
        Ok(()) => *rx.borrow(),
        Err(_sender_gone) => std::future::pending().await,
    }
}

/// Non-blocking companion to [`shutdown_code`]: the cancel code when
/// the watch has already fired, `None` when it has not (or never will
/// — a dropped sender means no outer cancel, same as `shutdown_code`).
fn shutdown_fired(shutdown: &Option<tokio::sync::watch::Receiver<i32>>) -> Option<i32> {
    let rx = shutdown.as_ref()?;
    match rx.has_changed() {
        Ok(true) => Some(*rx.borrow()),
        _ => None,
    }
}
