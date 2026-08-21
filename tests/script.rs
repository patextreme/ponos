//! Unit tests for the sandboxed Luau environment, require resolution, and
//! the task runtime primitives (with fake async ops).

use std::sync::Arc;

use mlua::FromLua as _;
use mlua::luau::Require as _;
use mlua::{Function, Lua, Value};

use ponos::config::Registry;
use ponos::render::{RenderOptions, Renderer};
use ponos::script::{self, RunConfig, require::ScriptRequirer};
use ponos::task::{TaskRegistry, spawn};

/// Build a Lua env exactly like production (sandbox + ponos table).
fn test_lua(script_dir: &std::path::Path) -> Lua {
    let cfg = RunConfig {
        script_path: script_dir.join("main.luau"),
        invocation_dir: script_dir.to_path_buf(),
        registry: Registry::from_parts(None, None).unwrap(),
        renderer: Arc::new(Renderer::new(RenderOptions::quiet())),
    };
    script::setup_lua(&cfg).unwrap()
}

fn tmpdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ponos-script-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// 6.1 sandbox
// ---------------------------------------------------------------------------

#[test]
fn sandboxed_globals_absent() {
    let dir = tmpdir("sandbox");
    std::fs::write(dir.join("main.luau"), "").unwrap();
    let lua = test_lua(&dir);
    let globals = lua.globals();

    for name in ["io", "debug", "package"] {
        let v: Value = globals.get(name).unwrap();
        assert!(matches!(v, Value::Nil), "{name} must be absent, got {v:?}");
    }

    // coroutine is restricted to `yield` (required by the embedded async
    // runtime); the scheduling functions are absent.
    let co: mlua::Table = globals.get("coroutine").unwrap();
    for name in ["create", "resume", "wrap", "status", "running"] {
        let v: Value = co.get(name).unwrap();
        assert!(matches!(v, Value::Nil), "coroutine.{name} must be absent");
    }
    assert!(co.get::<Function>("yield").is_ok());

    // raw-global escapes are poisoned: they raise on call
    for expr in ["loadstring('return 1')", "collectgarbage('count')"] {
        let err = lua.load(expr).eval::<()>().unwrap_err();
        assert!(!err.to_string().is_empty());
    }

    // os restricted to time/clock
    let os: mlua::Table = globals.get("os").unwrap();
    assert!(os.get::<Function>("time").is_ok());
    assert!(os.get::<Function>("clock").is_ok());
    for name in ["execute", "getenv", "remove", "rename", "exit", "date"] {
        let v: Value = os.get(name).unwrap();
        assert!(matches!(v, Value::Nil), "os.{name} must be absent");
    }
}

#[test]
fn print_passthrough_writes_stdout() {
    let dir = tmpdir("print");
    std::fs::write(dir.join("main.luau"), "").unwrap();
    let lua = test_lua(&dir);
    // print exists and callable; output goes to real stdout (assert via no-panic).
    let _: () = lua.load("print('hello from script')").eval().unwrap();
}

#[test]
fn curated_stdlib_present() {
    let dir = tmpdir("stdlib");
    std::fs::write(dir.join("main.luau"), "").unwrap();
    let lua = test_lua(&dir);
    for lib in ["string", "table", "math", "utf8", "bit32", "buffer"] {
        let v: Value = lua.globals().get(lib).unwrap();
        assert!(!matches!(v, Value::Nil), "{lib} must exist");
    }
}

// ---------------------------------------------------------------------------
// 6.2 require
// ---------------------------------------------------------------------------

#[test]
fn require_sibling_missing_and_cached() {
    let dir = tmpdir("require");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("main.luau"), "").unwrap();
    std::fs::write(dir.join("lib/util.luau"), "return { n = 41 + 1 }").unwrap();
    let lua = test_lua(&dir);
    let entry = format!("@{}/main.luau", dir.display());

    // sibling module loads
    let v: u32 = lua
        .load("local m = require('./lib/util') return m.n")
        .set_name(entry.clone())
        .eval()
        .unwrap();
    assert_eq!(v, 42);

    // caching: same require path returns the same module table
    let same: bool = lua
        .load("return require('./lib/util') == require('./lib/util')")
        .set_name(entry.clone())
        .eval()
        .unwrap();
    assert!(same, "second require must return the cached module");

    // missing module raises an error naming the unresolved path
    let err = lua
        .load("require('./lib/nope')")
        .set_name(entry.clone())
        .eval::<()>()
        .unwrap_err();
    assert!(
        err.to_string().contains("nope"),
        "missing-module error must name the path: {err}"
    );

    // escaping path rejected with a Lua error
    let err = lua
        .load("require('../outside')")
        .set_name(entry.clone())
        .eval::<()>()
        .unwrap_err();
    assert!(err.to_string().contains("escapes"), "{err}");

    // absolute path rejected
    let err = lua
        .load("require('/etc/passwd')")
        .set_name(entry)
        .eval::<()>()
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn requirer_unit_navigation() {
    // direct ScriptRequirer unit checks (also covered in require.rs tests)
    let dir = tmpdir("requirer");
    std::fs::write(dir.join("a.luau"), "").unwrap();
    let mut req = ScriptRequirer::new(dir.clone());
    req.reset(&format!("@{}/a.luau", dir.display())).unwrap();
    assert!(req.has_module()); // a.luau itself resolves
}

// ---------------------------------------------------------------------------
// 6.3 task runtime (fake async ops)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn task_runtime_semantics() {
    let dir = tmpdir("tasks");
    std::fs::write(dir.join("main.luau"), "").unwrap();
    let lua = test_lua(&dir);
    let registry = TaskRegistry::default();

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let mk = |ms: u64| -> Function {
                lua.load(format!(
                    "return function() ponos.sleep({ms}); return {ms} end"
                ))
                .eval()
                .unwrap()
            };
            let t1 = spawn(&lua, &registry, mk(30)).unwrap();
            let t2 = spawn(&lua, &registry, mk(10)).unwrap();
            let t3 = spawn(&lua, &registry, mk(20)).unwrap();

            let first = |m: mlua::MultiValue, lua: &Lua| -> u64 {
                u64::from_lua(m.into_iter().next().unwrap(), lua).unwrap()
            };
            let v1 = first(t1.await_result().await.unwrap(), &lua);
            let v2 = first(t2.await_result().await.unwrap(), &lua);
            let v3 = first(t3.await_result().await.unwrap(), &lua);
            assert_eq!((v1, v2, v3), (30, 10, 20));

            // await re-raises the task error
            let boom: Function = lua
                .load("return function() error('boom', 0) end")
                .eval()
                .unwrap();
            let tb = spawn(&lua, &registry, boom).unwrap();
            let err = tb.await_result().await.unwrap_err();
            assert!(err.to_string().contains("boom"), "{err}");
        })
        .await;
}

#[test]
fn spawn_from_lua_chunk() {
    let dir = tmpdir("spawn-lua");
    std::fs::write(dir.join("main.luau"), "").unwrap();
    let lua = test_lua(&dir);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    rt.block_on(local.run_until(async {
        let t: String = lua
            .load("local t = ponos.spawn(function() return 7 end) return type(t)")
            .set_name(format!("@{}/main.luau", dir.display()))
            .eval()
            .unwrap();
        assert_eq!(t, "table");
    }));
}
