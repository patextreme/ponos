//! Task runtime backing `ptah.spawn` / `ptah.join` / `ptah.parallel`.
//!
//! A task is a Lua function driven as a coroutine on the runtime's local
//! executor (mlua's async support). Completion state lives in `TaskState`,
//! shared between the Task userdata handed to scripts and the runtime's
//! end-of-run bookkeeping. Everything here is single-threaded (the Lua side
//! of the runtime lives on one `LocalSet`); only the completion signal is
//! tokio-native.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue};

/// Completion record of one task.
pub enum TaskResult {
    /// The function returned values.
    Value(MultiValue),
    /// The function raised; the error is re-raised at await sites.
    Error(mlua::Error),
}

/// Shared state of one spawned task.
pub struct TaskState {
    done: Cell<bool>,
    errored: Cell<bool>,
    /// The outcome was observed by the script via await/join/map.
    delivered: Cell<bool>,
    result: RefCell<Option<TaskResult>>,
    notify: tokio::sync::Notify,
}

impl Default for TaskState {
    fn default() -> Self {
        Self {
            done: Cell::new(false),
            errored: Cell::new(false),
            delivered: Cell::new(false),
            result: RefCell::new(None),
            notify: tokio::sync::Notify::new(),
        }
    }
}

impl TaskState {
    /// Record completion and wake awaiters.
    pub fn complete(&self, result: TaskResult) {
        if let TaskResult::Error(_) = &result {
            self.errored.set(true);
        }
        *self.result.borrow_mut() = Some(result);
        self.done.set(true);
        self.notify.notify_waiters();
    }

    pub fn is_done(&self) -> bool {
        self.done.get()
    }

    pub fn is_errored(&self) -> bool {
        self.errored.get()
    }

    /// Whether the script ever observed this task's outcome. An error that
    /// was never delivered by script end fails the run.
    pub fn is_delivered(&self) -> bool {
        self.delivered.get()
    }

    pub fn mark_delivered(&self) {
        self.delivered.set(true);
    }

    /// The error message, when the task raised (for end-of-run reporting).
    pub fn error_message(&self) -> Option<String> {
        match self.result.borrow().as_ref() {
            Some(TaskResult::Error(e)) => Some(display_error(e)),
            _ => None,
        }
    }

    /// Await completion, then take the result (marks the task delivered).
    /// Task errors are re-raised at the await site.
    pub async fn await_result(&self) -> mlua::Result<MultiValue> {
        if !self.done.get() {
            self.notify.notified().await;
        }
        self.delivered.set(true);
        match self.result.borrow_mut().take() {
            Some(TaskResult::Value(values)) => Ok(values),
            Some(TaskResult::Error(e)) => Err(e),
            None => Err(mlua::Error::runtime("task result already consumed")),
        }
    }
}

/// Registry of live tasks for end-of-run semantics.
#[derive(Default)]
pub struct TaskRegistry {
    tasks: RefCell<Vec<Rc<TaskState>>>,
}

impl TaskRegistry {
    pub fn register(&self, state: Rc<TaskState>) {
        self.tasks.borrow_mut().push(state);
    }

    /// States of tasks that have not completed yet.
    pub fn pending(&self) -> Vec<Rc<TaskState>> {
        self.tasks
            .borrow()
            .iter()
            .filter(|t| !t.is_done())
            .cloned()
            .collect()
    }

    /// Errors never delivered to the script (fail the run at script end).
    pub fn undelivered_errors(&self) -> Vec<String> {
        self.tasks
            .borrow()
            .iter()
            .filter(|t| t.is_errored() && !t.is_delivered())
            .filter_map(|t| t.error_message())
            .collect()
    }
}

/// Spawn a task: drive `f` as an async coroutine on the current local
/// executor and register its state with the run's registry.
pub fn spawn(lua: &Lua, registry: &TaskRegistry, f: Function) -> mlua::Result<Rc<TaskState>> {
    let state = Rc::new(TaskState::default());
    registry.register(state.clone());
    let fut = f.call_async::<MultiValue>(());
    let s = state.clone();
    tokio::task::spawn_local(async move {
        let result = match fut.await {
            Ok(values) => TaskResult::Value(values),
            Err(e) => TaskResult::Error(e),
        };
        s.complete(result);
    });
    let _ = lua;
    Ok(state)
}

/// Stringify an mlua error for reporting.
pub fn display_error(e: &mlua::Error) -> String {
    match e {
        mlua::Error::RuntimeError(msg) => msg.clone(),
        other => format!("{other}"),
    }
}
