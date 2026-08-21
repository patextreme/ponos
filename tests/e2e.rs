//! End-to-end Luau runtime tests: scripts drive the in-repo mock agent
//! through the full `ponos` namespace.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ponos::config::Registry;
use ponos::render::{RenderOptions, Renderer};
use ponos::script::{self, RunConfig, RunOutcome};

fn mock_agent() -> String {
    env!("CARGO_BIN_EXE_mock-agent").to_string()
}

fn tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ponos-e2e-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_script(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("main.luau");
    std::fs::write(&path, body).unwrap();
    path
}

fn run(script: &std::path::Path, dir: &std::path::Path) -> RunOutcome {
    run_with_registry(script, dir, Registry::from_parts(None, None).unwrap())
}

fn run_with_registry(
    script: &std::path::Path,
    dir: &std::path::Path,
    registry: Registry,
) -> RunOutcome {
    let cfg = RunConfig {
        script_path: script.to_path_buf(),
        invocation_dir: dir.to_path_buf(),
        registry,
        renderer: Arc::new(Renderer::new(RenderOptions::quiet())),
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(tokio::task::LocalSet::new().run_until(script::run(cfg)))
}

#[test]
fn full_prompt_turn_from_luau() {
    let dir = tmpdir("turn");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_CHUNKS = "Hel|lo", MOCK_USAGE = "5,10,2,3" }} }})
local s = agent:session({{ id = "reviewer" }})
local r = s:prompt("ignored")
assert(r.text == "Hello", "got " .. tostring(r.text))
assert(tostring(r) == "Hello")
assert(r.stop_reason == "end_turn")
assert(r.usage.input == 2 and r.usage.output == 3)
assert(s:label() == "<inline>/reviewer" or s:label():find("reviewer", 1, true))
s:close()
"#,
            mock = mock_agent()
        ),
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn default_session_labels() {
    let dir = tmpdir("labels");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}" }})
local a = agent:session()
local b = agent:session()
assert(a:label():match("/s1$"), a:label())
assert(b:label():match("/s2$"), b:label())
a:close()
b:close()
"#,
            mock = mock_agent()
        ),
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn unknown_agent_names_the_agent() {
    let dir = tmpdir("unknown");
    let script = write_script(&dir, "local a = ponos.agent('nope')");
    let out = run(&script, &dir);
    assert_eq!(out.code, 1);
    assert!(
        out.error.as_deref().unwrap_or("").contains("nope"),
        "{:?}",
        out.error
    );
}

#[test]
fn watchdog_cancel_is_control_flow() {
    let dir = tmpdir("watchdog");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_HANG = "1" }} }})
local s = agent:session()
local a = ponos.spawn(function() return s:prompt("slow") end)
ponos.sleep(100)
s:cancel()
local r = a:await()
assert(r.stop_reason == "cancelled", r.stop_reason)
s:close()
"#,
            mock = mock_agent()
        ),
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn timeout_is_catchable_error() {
    let dir = tmpdir("timeout");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_HANG = "1" }} }})
local s = agent:session()
local ok, err = pcall(function() return s:prompt("slow", {{ timeout_ms = 100 }}) end)
assert(not ok, "must time out")
assert(tostring(err):find("timed out", 1, true), tostring(err))
s:close()
"#,
            mock = mock_agent()
        ),
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn parallel_fanout_map_in_order_with_cap() {
    let dir = tmpdir("map");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_DELAY_MS = "100" }} }})
local s = agent:session()
local results = ponos.map({{"a", "b", "c", "d"}}, function(item)
    local r = s:prompt(item)
    return item .. ":" .. r.text
end, {{ concurrency = 2 }})
for i, entry in ipairs(results) do
    assert(entry.ok, entry.error)
end
assert(results[1].value == "a:a", results[1].value)
assert(results[4].value == "d:d", results[4].value)
s:close()
"#,
            mock = mock_agent()
        ),
    );
    let start = Instant::now();
    let out = run(&script, &dir);
    let elapsed = start.elapsed();
    assert_eq!(out.code, 0, "error: {:?}", out.error);
    // 4 turns, 2 at a time, 100ms each => at least 200ms total.
    assert!(
        elapsed >= Duration::from_millis(200),
        "concurrency cap not honored: {elapsed:?}"
    );
}

#[test]
fn join_contains_task_errors() {
    let dir = tmpdir("join");
    let script = write_script(
        &dir,
        r#"
local t1 = ponos.spawn(function() return 1 end)
local t2 = ponos.spawn(function() error("middle task failed", 0) end)
local t3 = ponos.spawn(function() ponos.sleep(50); return 3 end)
local outcomes = ponos.join({t1, t2, t3})
assert(outcomes[1].ok and outcomes[1].value == 1)
assert(not outcomes[2].ok and outcomes[2].error:find("middle task failed", 1, true))
assert(outcomes[3].ok and outcomes[3].value == 3)
"#,
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn script_end_waits_for_pending_spawn() {
    let dir = tmpdir("pending");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_DELAY_MS = "120" }} }})
local s = agent:session()
ponos.spawn(function() s:prompt("late") end)
-- main chunk returns immediately; ponos must wait for the pending task
"#,
            mock = mock_agent()
        ),
    );
    let start = Instant::now();
    let out = run(&script, &dir);
    let elapsed = start.elapsed();
    assert_eq!(out.code, 0, "error: {:?}", out.error);
    assert!(
        elapsed >= Duration::from_millis(110),
        "script end did not wait: {elapsed:?}"
    );
}

#[test]
fn explicit_exit_code_wins_and_tears_down() {
    let dir = tmpdir("exit");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_HANG = "1" }} }})
local s = agent:session()
ponos.spawn(function() s:prompt("pending") end)
ponos.sleep(50)
ponos.exit(3)
"#,
            mock = mock_agent()
        ),
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 3, "error: {:?}", out.error);
}

#[test]
fn uncaught_error_fails_run() {
    let dir = tmpdir("error");
    let script = write_script(&dir, "error('script exploded', 0)");
    let out = run(&script, &dir);
    assert_eq!(out.code, 1);
    assert!(
        out.error
            .as_deref()
            .unwrap_or("")
            .contains("script exploded"),
        "{:?}",
        out.error
    );
}

#[test]
fn never_retrieved_task_error_fails_run() {
    let dir = tmpdir("undelivered");
    let script = write_script(
        &dir,
        r#"
ponos.spawn(function() ponos.sleep(50); error("nobody saw this", 0) end)
ponos.spawn(function() ponos.sleep(10) end)
"#,
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 1);
    assert_eq!(out.undelivered_errors.len(), 1);
    assert!(
        out.undelivered_errors[0].contains("nobody saw this"),
        "{:?}",
        out.undelivered_errors
    );
}

#[test]
fn delivered_via_await_task_error_does_not_fail_run() {
    let dir = tmpdir("delivered");
    let script = write_script(
        &dir,
        r#"
local t = ponos.spawn(function() error("observed", 0) end)
pcall(function() return t:await() end)
"#,
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
    assert!(out.undelivered_errors.is_empty());
}

#[test]
fn default_session_cwd_is_invocation_dir() {
    // CLI spec "Session cwd defaults to invocation directory": a session
    // created without `cwd` runs in the invocation directory (the mock
    // echoes its session cwd).
    let dir = tmpdir("cwd");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_ECHO_CWD = "1" }} }})
local s = agent:session()
local r = s:prompt("where am I")
assert(r.text == "{expected}", "cwd was " .. tostring(r.text))
s:close()
"#,
            mock = mock_agent(),
            expected = dir.display()
        ),
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn two_agent_calls_same_name_give_independent_factories() {
    // Scripting spec "Agent and session API": two ponos.agent calls for the
    // same name return independent factory objects (independent s1/s2
    // counters, distinct sessions).
    let dir = tmpdir("factories");
    let project_config = format!("[agents.mock]\ncommand = \"{}\"\n", mock_agent());
    let registry = Registry::from_parts(None, Some(project_config.as_str())).unwrap();
    let script = write_script(
        &dir,
        r#"
local f1 = ponos.agent("mock")
local f2 = ponos.agent("mock")
assert(f1 ~= f2, "factories must be distinct objects")
local s1 = f1:session()
local s2 = f2:session()
assert(s1:label() == "mock/s1", s1:label())
assert(s2:label() == "mock/s1", s2:label())
assert(s1 ~= s2, "sessions must be distinct objects")
s1:close()
s2:close()
"#,
    );
    let out = run_with_registry(&script, &dir, registry);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn log_and_version_helpers() {
    let dir = tmpdir("helpers");
    let script = write_script(
        &dir,
        r#"
ponos.log("starting")
assert(type(ponos.version) == "string" and #ponos.version > 0)
assert(ponos.sleep ~= nil and ponos.spawn ~= nil and ponos.map ~= nil and ponos.join ~= nil)
"#,
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}
