//! The `ponos.*` namespace bindings and the object constructors handed
//! to scripts: task/session/agent-factory tables (plain tables with
//! closure methods; userdata metatables would be built lazily by mlua,
//! which re-reads the `coroutine` global we hide — so userdata is avoided
//! entirely).

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use mlua::LuaSerdeExt;
use mlua::{Function, Lua, MultiValue, Table, Value};

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionValue, SessionConfigSelectOption,
};

use ponos_core::config::AgentSpec;
use ponos_core::error::ExitSignal;
use ponos_core::events::SessionEvent;
use ponos_core::session::{SessionHandle, SessionOptions};
use ponos_core::task::{self, TaskState};

use super::state::runtime_state;

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
                // Remove by pid (session identity), not label: two
                // factories for one agent name can have live sessions
                // sharing a label, and this registry is the run-end
                // teardown list — a label match would unregister the
                // survivor and strand its subprocess.
                state.sessions.borrow_mut().retain(|s| s.pid != handle.pid);
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

                // The `config` session option is removed (it was a table
                // applied with unspecified `pairs()` order, which cannot
                // coexist with agents that re-derive dependent options).
                // The key itself is the removed API: reject it pre-spawn —
                // populated or empty — instead of silently ignoring old
                // scripts; the author migrates to sequential setConfig
                // calls in dependency order (driving options first).
                if opts.get::<Option<Table>>("config")?.is_some() {
                    return Err(mlua::Error::runtime(
                        "config session option was removed: a config table cannot express \
                         application order, which matters for agents with dependent options \
                         (e.g. opencode resets `effort` when `model` is set). Apply config \
                         with session:setConfig(...) after session creation — set driving \
                         options (like `model`) first",
                    ));
                }

                // Typed result contract: eager compilation so schema errors
                // fail at the author's line, before any subprocess spawns.
                let result =
                    match opts.get::<Option<Value>>("resultSchema")? {
                        Some(raw) => {
                            let json: serde_json::Value = lua.from_value(raw).map_err(|e| {
                                mlua::Error::runtime(format!("invalid result schema: {e}"))
                            })?;
                            Some(ponos_core::contract::ResultContract::compile(json).map_err(
                                |e| mlua::Error::runtime(format!("invalid result schema: {e}")),
                            )?)
                        }
                        None => None,
                    };

                state.sink.emit(
                    &label,
                    SessionEvent::Lifecycle {
                        message: format!("{label}: spawning agent"),
                    },
                );
                let handle = state
                    .transport
                    .start_session(
                        &spec,
                        SessionOptions {
                            cwd,
                            mcp_servers,
                            label: label.clone(),
                            result,
                        },
                        state.sink.clone(),
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
fn interp_lookup(var: &str) -> Option<String> {
    std::env::var(var).ok()
}

pub(super) fn bind_ponos(lua: &Lua) -> mlua::Result<()> {
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

    // ponos.parallel(items, fn, {concurrency}) -> outcome entries in item order
    let parallel = lua.create_async_function(
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
    ponos.set("parallel", parallel)?;

    // ponos.sleep(ms)
    let sleep = lua.create_async_function(|_lua, ms: u64| async move {
        tokio::time::sleep(Duration::from_millis(ms)).await;
        Ok(())
    })?;
    ponos.set("sleep", sleep)?;

    // ponos.log(msg)
    let log = lua.create_function(|lua, msg: String| {
        let state = runtime_state(lua)?;
        state.sink.script_log(&msg);
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
    ponos.set("version", ponos_core::VERSION)?;
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
