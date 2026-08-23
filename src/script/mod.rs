//! The Luau scripting environment: sandbox, curated stdlib, custom require,
//! the `ponos` namespace bindings, and the run loop with end-of-run
//! semantics (wait for outstanding tasks, teardown agents, exit codes).

pub mod require;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use mlua::LuaSerdeExt;
use mlua::{Function, Lua, LuaOptions, MultiValue, StdLib, Table, Value};

use crate::acp::{self, SessionHandle, SessionOptions};
use crate::config::{AgentSpec, Registry};
use crate::render::Renderer;
use crate::task::{self, TaskRegistry, TaskState};

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionValue, SessionConfigSelectOption,
};

use require::ScriptRequirer;

/// Signals `ponos.exit(code)`: unwinds the run; the code wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitSignal {
    pub code: i32,
}

impl std::fmt::Display for ExitSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ponos.exit({})", self.code)
    }
}

impl std::error::Error for ExitSignal {}

/// Everything the Lua environment needs, shared per run (single-threaded).
pub struct RuntimeState {
    pub registry: Registry,
    pub renderer: Arc<Renderer>,
    pub invocation_dir: PathBuf,
    pub script_root: PathBuf,
    pub tasks: Rc<TaskRegistry>,
    pub sessions: RefCell<Vec<SessionHandle>>,
    pub exit_code: Cell<Option<i32>>,
}

/// Configuration for one `ponos run`.
pub struct RunConfig {
    pub script_path: PathBuf,
    pub invocation_dir: PathBuf,
    pub registry: Registry,
    pub renderer: Arc<Renderer>,
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

fn runtime_state(lua: &Lua) -> mlua::Result<Rc<RuntimeState>> {
    lua.app_data_ref::<Rc<RuntimeState>>()
        .map(|s| Rc::clone(&s))
        .ok_or_else(|| mlua::Error::runtime("ponos runtime state missing"))
}

// ---------------------------------------------------------------------------
// Object constructors (plain tables with closure methods; userdata
// metatables would be built lazily by mlua, which re-reads the `coroutine`
// global we hide — so we avoid userdata entirely)
// ---------------------------------------------------------------------------

fn new_task_obj(lua: &Lua, state: Rc<TaskState>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let s = state.clone();
    t.set(
        "await",
        lua.create_async_function(move |_lua, _self: Table| {
            let s = s.clone();
            async move { s.await_result().await }
        })?,
    )?;
    Ok(t)
}

fn new_session_obj(lua: &Lua, handle: SessionHandle) -> mlua::Result<Table> {
    let t = lua.create_table()?;

    let prompt_handle = handle.clone();
    t.set(
        "prompt",
        lua.create_async_function(
            move |lua, (_self, text, opts): (Table, String, Option<Table>)| {
                let handle = prompt_handle.clone();
                async move {
                    let timeout = match &opts {
                        Some(t) => t
                            .get::<Option<u64>>("timeoutMs")?
                            .map(Duration::from_millis),
                        None => None,
                    };
                    // Turn serialization lives in the session handle.
                    let outcome = handle
                        .prompt(text, timeout)
                        .await
                        .map_err(|e| mlua::Error::runtime(e.to_string()))?;

                    let result = lua.create_table()?;
                    result.set("text", outcome.text.clone())?;
                    result.set("stopReason", outcome.stop_reason)?;
                    let usage = lua.create_table()?;
                    usage.set("input", outcome.usage.input)?;
                    usage.set("cacheRead", outcome.usage.cache_read)?;
                    usage.set("cacheWrite", outcome.usage.cache_write)?;
                    usage.set("output", outcome.usage.output)?;
                    result.set("usage", usage)?;
                    // The turn's last accepted typed submission as a Luau
                    // value, or nil when there was none. JSON null also
                    // arrives as nil (mlua's default would produce its
                    // null userdata sentinel instead).
                    let submitted = outcome.result.as_ref().map_or(Value::Nil, |json| {
                        lua.to_value_with(
                            json,
                            mlua::serde::ser::Options::new()
                                .serialize_none_to_null(false)
                                .serialize_unit_to_null(false),
                        )
                        .unwrap_or(Value::Nil)
                    });
                    result.set("result", submitted)?;

                    let meta = lua.create_table()?;
                    meta.set(
                        "__tostring",
                        lua.create_function(|_lua, t: Table| {
                            let text: String = t.get("text")?;
                            Ok(text)
                        })?,
                    )?;
                    result.set_metatable(Some(meta))?;
                    Ok(result)
                }
            },
        )?,
    )?;

    let cancel_handle = handle.clone();
    t.set(
        "cancel",
        lua.create_function(move |_lua, _self: Table| {
            cancel_handle.cancel();
            Ok(())
        })?,
    )?;

    let label = handle.label.clone();
    t.set(
        "label",
        lua.create_function(move |_lua, _self: Table| Ok(label.clone()))?,
    )?;

    let config_handle = handle.clone();
    t.set(
        "configOptions",
        lua.create_function(move |lua, _self: Table| {
            let options = config_handle.config_options();
            config_options_table(lua, &options)
        })?,
    )?;

    let set_config_handle = handle.clone();
    t.set(
        "setConfig",
        lua.create_async_function(move |_lua, (_self, id, value): (Table, String, Value)| {
            let handle = set_config_handle.clone();
            async move {
                // Value typing happens before anything is sent: a Luau
                // string is a select value id, a boolean is a boolean
                // option value, anything else is a script error.
                let wire_value = match value {
                    Value::String(s) => {
                        let id = s.to_str()?.to_string();
                        SessionConfigOptionValue::value_id(id)
                    }
                    Value::Boolean(b) => SessionConfigOptionValue::boolean(b),
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "setConfig value must be a string (select value id) or boolean, \
                                 got {}",
                            other.type_name()
                        )));
                    }
                };
                handle
                    .set_config(id, wire_value)
                    .await
                    .map_err(mlua::Error::runtime)
            }
        })?,
    )?;

    let close_handle = handle.clone();
    t.set(
        "close",
        lua.create_async_function(move |lua, _self: Table| {
            let handle = close_handle.clone();
            async move {
                let state = runtime_state(&lua)?;
                handle.close();
                handle.join().await;
                state
                    .sessions
                    .borrow_mut()
                    .retain(|s| s.label != handle.label);
                Ok(())
            }
        })?,
    )?;

    Ok(t)
}

/// Convert the session's config-option state to a Luau array of option
/// tables: `{ id, name, type ("select"|"boolean"), currentValue, category?,
/// options? }` — select entries carry an `options` list of
/// `{ id, name, description? }` choices (grouped selects are flattened);
/// `category` is set only when the agent provides one (UX hint only).
fn config_options_table(lua: &Lua, options: &[SessionConfigOption]) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (i, opt) in options.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("id", opt.id.0.to_string())?;
        e.set("name", opt.name.clone())?;
        match &opt.kind {
            SessionConfigKind::Select(s) => {
                e.set("type", "select")?;
                e.set("currentValue", s.current_value.0.to_string())?;
                let choices = lua.create_table()?;
                for (n, choice) in flatten_select_options(&s.options).into_iter().enumerate() {
                    let c = lua.create_table()?;
                    c.set("id", choice.value.0.to_string())?;
                    c.set("name", choice.name.clone())?;
                    if let Some(d) = &choice.description {
                        c.set("description", d.clone())?;
                    }
                    choices.raw_set(n + 1, c)?;
                }
                e.set("options", choices)?;
            }
            SessionConfigKind::Boolean(b) => {
                e.set("type", "boolean")?;
                e.set("currentValue", b.current_value)?;
            }
            _ => continue, // unknown option kinds are skipped
        }
        if let Some(category) = &opt.category
            && let serde_json::Value::String(name) =
                serde_json::to_value(category).unwrap_or(serde_json::Value::Null)
        {
            e.set("category", name)?;
        }
        t.raw_set(i + 1, e)?;
    }
    Ok(t)
}

/// Flatten a select option's choices (grouped selects contribute every
/// group's options, in order).
fn flatten_select_options(
    options: &agent_client_protocol::schema::v1::SessionConfigSelectOptions,
) -> Vec<&SessionConfigSelectOption> {
    use agent_client_protocol::schema::v1::SessionConfigSelectOptions;
    match options {
        SessionConfigSelectOptions::Ungrouped(list) => list.iter().collect(),
        SessionConfigSelectOptions::Grouped(groups) => {
            groups.iter().flat_map(|g| g.options.iter()).collect()
        }
        _ => Vec::new(),
    }
}

fn new_agent_factory(lua: &Lua, name: String, spec: AgentSpec) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let name_rc = Rc::new(name);
    let spec_rc = Rc::new(spec);
    let counter = Rc::new(Cell::new(0u64));
    t.set(
        "session",
        lua.create_async_function(move |lua, (_self, opts): (Table, Option<Table>)| {
            let name = name_rc.clone();
            let spec = spec_rc.clone();
            let counter = counter.clone();
            async move {
                let state = runtime_state(&lua)?;
                let opts = match opts {
                    Some(t) => t,
                    None => lua.create_table()?,
                };

                let id: Option<String> = opts.get("id")?;
                let n = counter.get() + 1;
                counter.set(n);
                let id = id.unwrap_or_else(|| format!("s{n}"));
                let label = format!("{}/{}", name, id);

                let cwd: Option<String> = opts.get("cwd")?;
                let cwd = match cwd {
                    Some(dir) => {
                        let p = Path::new(&dir);
                        if p.is_absolute() {
                            p.to_path_buf()
                        } else {
                            state.invocation_dir.join(p)
                        }
                    }
                    None => state.invocation_dir.clone(),
                };

                let mut mcp_servers = Vec::new();
                let raw: Option<Value> = opts.get("mcpServers")?;
                if let Some(raw) = raw {
                    let json: serde_json::Value = lua.from_value(raw)?;
                    mcp_servers = serde_json::from_value(json).map_err(|e| {
                        mlua::Error::runtime(format!("invalid mcpServers entry: {e}"))
                    })?;
                }

                // Typed result contract: eager compilation so schema errors
                // fail at the author's line, before any subprocess spawns.
                let result = match opts.get::<Option<Value>>("result")? {
                    Some(raw) => {
                        let json: serde_json::Value = lua.from_value(raw).map_err(|e| {
                            mlua::Error::runtime(format!("invalid result schema: {e}"))
                        })?;
                        Some(
                            crate::result_contract::ResultContract::compile(json).map_err(|e| {
                                mlua::Error::runtime(format!("invalid result schema: {e}"))
                            })?,
                        )
                    }
                    None => None,
                };

                state
                    .renderer
                    .lifecycle(&format!("{label}: spawning agent"));
                let handle = acp::start_session(
                    &spec,
                    SessionOptions {
                        cwd,
                        mcp_servers,
                        label: label.clone(),
                        result,
                    },
                    state.renderer.clone(),
                )
                .await
                .map_err(|e| mlua::Error::runtime(e.to_string()))?;
                state.sessions.borrow_mut().push(handle.clone());

                new_session_obj(&lua, handle)
            }
        })?,
    )?;
    Ok(t)
}

// ---------------------------------------------------------------------------
// ponos.* bindings
// ---------------------------------------------------------------------------

fn interp_lookup(var: &str) -> Option<String> {
    std::env::var(var).ok()
}

fn bind_ponos(lua: &Lua) -> mlua::Result<()> {
    let ponos = lua.create_table()?;

    // ponos.agent(name_or_spec)
    let agent = lua.create_async_function(|lua, spec: Value| async move {
        let state = runtime_state(&lua)?;
        let resolved = match &spec {
            Value::String(name) => {
                let name = name.to_str()?.to_string();
                state
                    .registry
                    .resolve_with(&name, &interp_lookup)
                    .map_err(|e| mlua::Error::runtime(e.to_string()))?
            }
            Value::Table(t) => {
                let args: Option<Vec<String>> = t.get("args")?;
                let env: Option<std::collections::BTreeMap<String, String>> = t.get("env")?;
                AgentSpec {
                    command: t.get("command")?,
                    args: args.unwrap_or_default(),
                    env: env.unwrap_or_default(),
                }
                .interpolate(&interp_lookup)
            }
            other => {
                // mlua's `BadArgument::cause` is an `Arc<Error>`; without the
                // `send` feature `Error` is !Sync, which is fine here.
                #[allow(clippy::arc_with_non_send_sync)]
                let cause = std::sync::Arc::new(mlua::Error::runtime(format!(
                    "expected string or table, got {}",
                    other.type_name()
                )));
                return Err(mlua::Error::BadArgument {
                    to: Some("ponos.agent".into()),
                    pos: 1,
                    name: Some("name_or_spec".into()),
                    cause,
                });
            }
        };
        let name = match &spec {
            Value::String(name) => name.to_str()?.to_string(),
            _ => resolved.command.clone(),
        };
        new_agent_factory(&lua, name, resolved)
    })?;
    ponos.set("agent", agent)?;

    // ponos.spawn(fn)
    let spawn = lua.create_function(|lua, f: Function| {
        let state = runtime_state(lua)?;
        let task_state = task::spawn(lua, &state.tasks, f)?;
        Ok(new_task_obj(lua, task_state))
    })?;
    ponos.set("spawn", spawn)?;

    // ponos.join({task, ...}) -> outcome entries
    let join = lua.create_async_function(|lua, tasks: Table| async move {
        let outcomes = lua.create_table()?;
        let len = tasks.raw_len();
        for i in 1..=len {
            let entry_task: Table = tasks.get(i)?;
            let await_fn: Function = entry_task
                .get("await")
                .map_err(|_| mlua::Error::runtime("join expects task objects"))?;
            let res: mlua::Result<MultiValue> = await_fn.call_async((entry_task.clone(),)).await;
            let entry = outcome_entry(&lua, res)?;
            outcomes.raw_set(i, entry)?;
        }
        Ok(outcomes)
    })?;
    ponos.set("join", join)?;

    // ponos.map(items, fn, {concurrency}) -> outcome entries in item order
    let map = lua.create_async_function(
        |lua, (items, f, opts): (Table, Function, Option<Table>)| async move {
            let state = runtime_state(&lua)?;
            let concurrency: usize = match &opts {
                Some(t) => {
                    let c: Option<usize> = t.get("concurrency")?;
                    c.unwrap_or(usize::MAX)
                }
                None => usize::MAX,
            };
            let concurrency = concurrency.max(1);

            let mut item_values: Vec<Value> = Vec::new();
            for i in 1..=items.raw_len() {
                item_values.push(items.get(i)?);
            }

            let outcomes = lua.create_table()?;
            let mut idx = 0;
            for chunk in item_values.chunks(concurrency) {
                // Launch the chunk concurrently.
                let mut states = Vec::new();
                for item in chunk {
                    let state_rc = Rc::new(TaskState::default());
                    state.tasks.register(state_rc.clone());
                    let fut = f.call_async::<MultiValue>(item.clone());
                    let s = state_rc.clone();
                    tokio::task::spawn_local(async move {
                        let result = match fut.await {
                            Ok(v) => task::TaskResult::Value(v),
                            Err(e) => task::TaskResult::Error(e),
                        };
                        s.complete(result);
                    });
                    states.push(state_rc);
                }
                // Await the chunk; errors become outcome entries, not failures.
                for s in states {
                    let res = s.await_result().await;
                    idx += 1;
                    let entry = outcome_entry(&lua, res)?;
                    outcomes.raw_set(idx, entry)?;
                }
            }
            Ok(outcomes)
        },
    )?;
    ponos.set("map", map)?;

    // ponos.sleep(ms)
    let sleep = lua.create_async_function(|_lua, ms: u64| async move {
        tokio::time::sleep(Duration::from_millis(ms)).await;
        Ok(())
    })?;
    ponos.set("sleep", sleep)?;

    // ponos.log(msg)
    let log = lua.create_function(|lua, msg: String| {
        let state = runtime_state(lua)?;
        state.renderer.script_log(&msg);
        Ok(())
    })?;
    ponos.set("log", log)?;

    // ponos.exit(code)
    let exit = lua.create_function(|lua, code: Option<i32>| {
        let state = runtime_state(lua)?;
        let code = code.unwrap_or(0);
        state.exit_code.set(Some(code));
        Err::<(), _>(mlua::Error::external(ExitSignal { code }))
    })?;
    ponos.set("exit", exit)?;

    // ponos.version (read-only)
    ponos.set("version", crate::VERSION)?;
    ponos.set_readonly(true);

    let globals = lua.globals();
    globals.set("ponos", ponos)?;
    Ok(())
}

/// Build a `{ ok = true, value = v }` / `{ ok = false, error = msg }` entry.
/// Multi-value task results contribute their first value.
fn outcome_entry(lua: &Lua, res: mlua::Result<MultiValue>) -> mlua::Result<Table> {
    let entry = lua.create_table()?;
    match res {
        Ok(values) => {
            let value = values.into_iter().next().unwrap_or(Value::Nil);
            entry.set("ok", true)?;
            entry.set("value", value)?;
        }
        Err(e) => {
            entry.set("ok", false)?;
            entry.set("error", task::display_error(&e))?;
        }
    }
    Ok(entry)
}

// ---------------------------------------------------------------------------
// Environment setup
// ---------------------------------------------------------------------------

/// Create the sandboxed Luau environment for a run.
pub fn setup_lua(cfg: &RunConfig) -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::TABLE
            | StdLib::OS
            | StdLib::STRING
            | StdLib::UTF8
            | StdLib::BIT
            | StdLib::BUFFER
            | StdLib::MATH,
        LuaOptions::default(),
    )?;
    lua.sandbox(true)?;

    let globals = lua.globals();

    // os: keep only time and clock.
    let os = globals.get::<Table>("os")?;
    let os_safe = lua.create_table()?;
    os_safe.set("time", os.get::<Function>("time")?)?;
    os_safe.set("clock", os.get::<Function>("clock")?)?;
    globals.set("os", os_safe)?;

    // require: relative to the script tree only. Canonicalize the entry
    // path so the sandbox root lives in the same absolute namespace as
    // chunk names (`@/abs/...`, set in `run`): the escape guard compares
    // the two, and a relative root would reject every require made by a
    // script invoked through a relative path (e.g. `ponos run dir/s.luau`).
    let entry = std::fs::canonicalize(&cfg.script_path).unwrap_or_else(|_| cfg.script_path.clone());
    let script_root = entry
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let require_fn = lua.create_require_function(ScriptRequirer::new(script_root.clone()))?;
    globals.set("require", require_fn)?;

    let state = Rc::new(RuntimeState {
        registry: cfg.registry.clone(),
        renderer: cfg.renderer.clone(),
        invocation_dir: cfg.invocation_dir.clone(),
        script_root,
        tasks: Rc::new(TaskRegistry::default()),
        sessions: RefCell::new(Vec::new()),
        exit_code: Cell::new(None),
    });
    lua.set_app_data(state);

    bind_ponos(&lua)?;

    // mlua's async machinery re-reads the global `coroutine` whenever an
    // async callback is created (some of ours are created lazily at
    // runtime), so the global must remain a table with a real `yield`.
    // Restrict it to exactly that: no create/resume/wrap (concurrency is
    // ponos.spawn's job). Documented deviation from the curated-stdlib list.
    let globals = lua.globals();
    let coroutine = globals.get::<Table>("coroutine")?;
    let coroutine_safe = lua.create_table()?;
    coroutine_safe.set("yield", coroutine.get::<Function>("yield")?)?;
    globals.set("coroutine", coroutine_safe)?;
    let poison = lua.create_function(|_lua, ()| -> mlua::Result<()> {
        Err(mlua::Error::runtime(
            "this global is not available in ponos scripts",
        ))
    })?;
    globals.set("loadstring", poison.clone())?;
    globals.set("collectgarbage", poison)?;

    Ok(lua)
}

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
        code: if undelivered.is_empty() { 0 } else { 1 },
        error: None,
        undelivered_errors: undelivered,
    }
}
