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
assert(r.stopReason == "end_turn")
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
assert(r.stopReason == "cancelled", r.stopReason)
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
local ok, err = pcall(function() return s:prompt("slow", {{ timeoutMs = 100 }}) end)
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
fn parallel_fanout_in_order_with_cap() {
    let dir = tmpdir("parallel");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{ MOCK_DELAY_MS = "100" }} }})
local s = agent:session()
local results = ponos.parallel({{"a", "b", "c", "d"}}, function(item)
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
fn plain_session_outcome_has_nil_result() {
    // Scripting spec "Prompt returns a result table": the `result` field is
    // nil on sessions that declared no typed-result contract.
    let dir = tmpdir("plain-nil");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}" }})
local s = agent:session()
local r = s:prompt("hi")
assert(r.text == "hi", r.text)
assert(r.result == nil, "plain session result must be nil")
assert(r.stopReason == "end_turn")
s:close()
"#,
            mock = mock_agent()
        ),
    );
    let out = run(&script, &dir);
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
assert(ponos.sleep ~= nil and ponos.spawn ~= nil and ponos.parallel ~= nil and ponos.join ~= nil)
"#,
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

// ---------------------------------------------------------------------------
// Constructor `config` option (session-config-options capability)
// ---------------------------------------------------------------------------

/// Select `model` + boolean `fast` options the mock advertises and mutates.
const CONFIG_OPTIONS_JSON: &str = r#"[{"id":"model","name":"Model","type":"select","currentValue":"opus","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}]},{"id":"fast","name":"Fast mode","type":"boolean","currentValue":false}]"#;

#[test]
fn constructor_config_applies_before_first_prompt() {
    // session-config-options spec "Config applied at session creation":
    // every entry is applied before the constructor returns (folded into
    // the live state), so the first prompt runs under the new settings;
    // a later setConfig composes as usual.
    let dir = tmpdir("cfg-ctor");
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{ command = "{mock}", env = {{
    MOCK_CONFIG_OPTIONS = '{json}',
    MOCK_CONFIG_ECHO = "model",
}} }})
local s = agent:session({{ config = {{ model = "haiku", fast = true }} }})
local options = s:configOptions()
assert(options[1].currentValue == "haiku", tostring(options[1].currentValue))
assert(options[2].currentValue == true, tostring(options[2].currentValue))
local r = s:prompt("go")
assert(r.text == "haiku", "first prompt must run under the constructor config: " .. r.text)
s:setConfig("model", "opus")
local r2 = s:prompt("again")
assert(r2.text == "opus", "later setConfig must compose: " .. r2.text)
s:close()
"#,
            mock = mock_agent(),
            json = CONFIG_OPTIONS_JSON
        ),
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

#[test]
fn constructor_config_bad_value_fails_before_spawn() {
    // session-config-options spec "Non-string-or-boolean value fails
    // before spawn": the error names the invalid entry and, like the
    // schema-compile path, fires before any subprocess spawns — the
    // command does not exist, so a spawn would name the command instead.
    let dir = tmpdir("cfg-bad-value");
    let script = write_script(
        &dir,
        r#"
local agent = ponos.agent({ command = "/nonexistent/ponos-test-agent" })
local ok, err = pcall(function()
    return agent:session({ config = { model = 42 } })
end)
assert(not ok, "session() must fail on a non-string-or-boolean config value")
local msg = tostring(err)
assert(msg:find("config.model", 1, true), "must name the entry: " .. msg)
assert(msg:find("boolean", 1, true), "must name the allowed types: " .. msg)
assert(not msg:find("/nonexistent", 1, true), "must fail before spawning: " .. msg)
"#,
    );
    let out = run(&script, &dir);
    assert_eq!(out.code, 0, "error: {:?}", out.error);
}

/// Count live processes whose cmdline contains `needle` (Linux /proc
/// scan; the mock ignores extra args, so a unique token tags exactly the
/// agent spawned for one session).
fn count_processes(needle: &str) -> usize {
    let mut n = 0;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(raw) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        if raw
            .split(|b| *b == 0)
            .any(|arg| std::str::from_utf8(arg).is_ok_and(|s| s.contains(needle)))
        {
            n += 1;
        }
    }
    n
}

/// Poll (20 ms) up to 5 s until `count_processes(needle) == want`.
fn wait_for_processes(needle: &str, want: usize, what: &str) {
    for _ in 0..250 {
        if count_processes(needle) == want {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "expected {what} (count {want}) for processes tagged {needle:?}, got {}",
        count_processes(needle)
    );
}

#[test]
fn constructor_config_agent_rejection_tears_down_the_agent() {
    // session-config-options spec "Agent rejection fails the constructor":
    // the error carries the config id and the agent's message, and the
    // agent subprocess is torn down by the constructor — observed live:
    // the tagged mock appears, then disappears while the script is still
    // sleeping, so the end-of-run sweep cannot be the reaper. The mock
    // holds the rejection back for 400 ms (MOCK_CONFIG_REJECT_DELAY_MS),
    // guaranteeing a wide, observable alive window.
    let dir = tmpdir("cfg-reject");
    let token = format!("--ponos-e2e-reject-token-{}", std::process::id());
    let script = write_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({{
    command = "{mock}",
    args = {{ "{token}" }},
    env = {{
        MOCK_CONFIG_OPTIONS = '{json}',
        MOCK_CONFIG_REJECT = "model",
        MOCK_CONFIG_REJECT_DELAY_MS = "400",
    }},
}})
local ok, err = pcall(function()
    return agent:session({{ config = {{ model = "haiku" }} }})
end)
assert(not ok, "session() must fail when the agent rejects the value")
local msg = tostring(err)
assert(msg:find("model", 1, true), "must name the config id: " .. msg)
assert(msg:find("rejects config id model", 1, true), "must carry the agent message: " .. msg)
ponos.sleep(3000)
"#,
            mock = mock_agent(),
            token = token,
            json = CONFIG_OPTIONS_JSON
        ),
    );
    let run_dir = dir.clone();
    let run_script = script.clone();
    let runner = std::thread::spawn(move || run(&run_script, &run_dir));
    wait_for_processes(&token, 1, "agent spawned before the rejected set");
    wait_for_processes(&token, 0, "agent torn down by the constructor");
    // The teardown was observed while the script was still in its sleep:
    // the constructor reaped the process, not the end-of-run sweep.
    assert!(
        !runner.is_finished(),
        "agent vanished before the script finished — the run-end sweep would be the reaper otherwise"
    );
    let out = runner.join().unwrap();
    assert_eq!(out.code, 0, "error: {:?}", out.error);
    assert_eq!(count_processes(&token), 0, "no process may survive the run");
}

#[test]
fn closing_one_session_keeps_same_label_survivor_in_the_run_end_sweep() {
    // Two factories for the same agent name both label their first
    // session `mock/s1` (scripting spec "Independent factories"), and
    // the session registry is the run-end teardown list — removal must
    // be by identity (pid), never by label, or closing one sibling
    // unregisters the survivor too. The survivor's mock parks on stdin
    // EOF (MOCK_EOF_LINGER), so EOF can never be the reaper here: the
    // sweep must explicitly kill the process group. (Today the driver's
    // last-handle-drop reap backstops a registry miss — this test keeps
    // that contract observable end-to-end instead of relying on it.)
    let dir = tmpdir("close-shared-label");
    let token_a = format!("--ponos-e2e-close-a-{}", std::process::id());
    let token_b = format!("--ponos-e2e-close-b-{}", std::process::id());
    let script = write_script(
        &dir,
        &format!(
            r#"
local a = ponos.agent({{ command = "{mock}", args = {{ "{token_a}" }} }})
local b = ponos.agent({{ command = "{mock}", args = {{ "{token_b}" }}, env = {{ MOCK_EOF_LINGER = "1" }} }})
local sa = a:session()
local sb = b:session()
assert(sa:label() == sb:label(), "both sessions must share a label for this regression")
sa:close()
ponos.sleep(2000)
local r = sb:prompt("hi")
assert(type(r.text) == "string", "survivor must stay usable after the sibling close")
"#,
            mock = mock_agent(),
            token_a = token_a,
            token_b = token_b
        ),
    );
    let run_dir = dir.clone();
    let run_script = script.clone();
    let runner = std::thread::spawn(move || run(&run_script, &run_dir));
    wait_for_processes(&token_a, 0, "closed session reaped by close()");
    wait_for_processes(&token_b, 1, "same-label survivor alive mid-run");
    assert!(
        !runner.is_finished(),
        "observation must happen while the script is still running"
    );
    let out = runner.join().unwrap();
    assert_eq!(out.code, 0, "error: {:?}", out.error);
    // The script never closed the survivor: the run-end sweep must reap
    // it even though a same-label sibling was closed earlier.
    wait_for_processes(&token_b, 0, "survivor reaped by the run-end sweep");
}
