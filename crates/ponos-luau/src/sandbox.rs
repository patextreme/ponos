//! Sandbox setup: create the sandboxed Luau environment for a run —
//! curated stdlib, the custom `require`, the ponos runtime state, and the
//! `ponos` namespace — with the documented `coroutine` deviation and the
//! poison globals.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use mlua::{Function, Lua, LuaOptions, StdLib, Table};

use ponos_core::task::TaskRegistry;

use crate::require::ScriptRequirer;

use super::bindings::bind_ponos;
use super::state::{RunConfig, RuntimeState};

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

    // require: relative to the requiring file, with no boundary. Canonicalize
    // the entry path so the requirer root lives in the same absolute
    // namespace as chunk names (`@/abs/...`, set in `run`): a relative root
    // would mis-resolve every require made by a script invoked through a
    // relative path (e.g. `ponos run dir/s.luau`).
    let entry = std::fs::canonicalize(&cfg.script_path).unwrap_or_else(|_| cfg.script_path.clone());
    let script_root = entry
        .parent()
        .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
    let require_fn = lua.create_require_function(ScriptRequirer::new(script_root))?;
    globals.set("require", require_fn)?;

    let state = Rc::new(RuntimeState {
        registry: cfg.registry.clone(),
        sink: cfg.renderer.clone(),
        transport: cfg.transport.clone(),
        invocation_dir: cfg.invocation_dir.clone(),
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
