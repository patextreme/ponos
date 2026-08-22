//! Typed-results integration suite: the `ponos __bridge` subcommand, the
//! injected submit tool, in-turn validation retry, slot semantics,
//! degradation, and concurrency. Script-level scenarios run the real CLI
//! binary (the injected server's command is `current_exe()`, which must be
//! the `ponos` binary, not a test harness); the bridge is exercised
//! directly against a stub result socket with an rmcp client.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn ponos_bin() -> &'static str {
    env!("CARGO_BIN_EXE_ponos")
}

fn mock_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mock-agent")
}

// ---------------------------------------------------------------------------
// Script harness: run a Luau script through the real binary with a mock
// agent whose env is provided inline.
// ---------------------------------------------------------------------------

fn tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ponos-typed-{}-{name}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mock_agent(env: &[(&str, &str)]) -> String {
    let mut spec = format!("{{ command = \"{}\", env = {{", mock_bin());
    for (k, v) in env {
        spec.push_str(&format!("\n\t[\"{k}\"] = [==[{v}]==],"));
    }
    spec.push_str("\n} }");
    spec
}

/// Run a script through the real binary; returns (stdout, stderr, success).
fn run_script(dir: &Path, body: &str, verbose: bool) -> (String, String, bool) {
    let script = dir.join("main.luau");
    std::fs::write(&script, body).unwrap();
    let mut cmd = Command::new(ponos_bin());
    cmd.arg("run").arg(&script).current_dir(dir);
    if verbose {
        cmd.arg("--verbose");
    }
    let output = cmd.output().expect("run ponos");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

fn assert_ok(dir: &Path, body: &str) -> (String, String) {
    let (stdout, stderr, ok) = run_script(dir, body, false);
    assert!(ok, "script failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");
    (stdout, stderr)
}

// ---------------------------------------------------------------------------
// Task 5.1: the bridge subcommand, driven by an rmcp client against a stub
// result socket.
// ---------------------------------------------------------------------------

/// A hand-rolled stand-in for ponos-main's side of the result channel:
/// accepts submits, validates `{ verdict: string, score: integer? }`, and
/// writes verdicts.
fn spawn_stub_listener() -> (std::thread::JoinHandle<()>, PathBuf) {
    use std::os::unix::net::UnixListener as StdListener;
    let dir = std::env::temp_dir().join(format!(
        "ponos-bridge-stub-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stub.sock");
    let listener = StdListener::bind(&path).unwrap();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut stream = stream;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            let request: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            let value = &request["value"];
            let verdict = if value.get("verdict").and_then(|v| v.as_str()).is_some() {
                serde_json::json!({ "ok": true })
            } else {
                serde_json::json!({
                    "ok": false,
                    "errors": ["\"verdict\" is a required property (at )"]
                })
            };
            writeln!(stream, "{verdict}").unwrap();
            stream.flush().unwrap();
        }
    });
    (handle, path)
}

#[tokio::test]
async fn bridge_subcommand_round_trips_submits_and_violations() {
    use rmcp::ServiceExt;
    use rmcp::model::CallToolRequestParams;
    use rmcp::transport::TokioChildProcess;

    let (_stub, path) = spawn_stub_listener();
    let schema =
        r#"{"type":"object","properties":{"verdict":{"type":"string"}},"required":["verdict"]}"#;

    // rmcp client speaking MCP over the bridge's stdio (spawning it
    // exactly as a real agent would).
    let mut command = tokio::process::Command::new(ponos_bin());
    command
        .arg("__bridge")
        .env("PONOS_BRIDGE_ADDR", &path)
        .env("PONOS_RESULT_SCHEMA", schema)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let transport = TokioChildProcess::new(command).expect("spawn bridge client-side");
    let mut client = ().serve(transport).await.expect("bridge handshake");

    // Exactly one tool, named result_submit, wrapping the declared schema
    // under `value`.
    let tools = client.list_tools(None).await.expect("list tools");
    assert_eq!(tools.tools.len(), 1, "{:?}", tools.tools);
    let tool = &tools.tools[0];
    assert_eq!(tool.name, "result_submit");
    assert_eq!(
        tool.input_schema["properties"]["value"],
        serde_json::json!({
            "type": "object",
            "properties": { "verdict": { "type": "string" } },
            "required": ["verdict"]
        })
    );
    assert_eq!(tool.input_schema["required"], serde_json::json!(["value"]));

    // Valid submit: accepted.
    let mut args = serde_json::Map::new();
    args.insert(
        "value".into(),
        serde_json::json!({ "verdict": "approve", "score": 8 }),
    );
    let result = client
        .call_tool(CallToolRequestParams::new("result_submit").with_arguments(args))
        .await
        .expect("call tool");
    assert_ne!(result.is_error, Some(true), "{:?}", result);

    // Invalid submit: tool error naming the violation.
    let mut args = serde_json::Map::new();
    args.insert("value".into(), serde_json::json!({ "score": 8 }));
    let result = client
        .call_tool(CallToolRequestParams::new("result_submit").with_arguments(args))
        .await
        .expect("call tool");
    assert_eq!(result.is_error, Some(true), "{:?}", result);
    let text: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect();
    assert!(text.contains("verdict"), "violation text: {text}");

    // Unknown tool: tool error, not a protocol crash.
    let result = client
        .call_tool(CallToolRequestParams::new("other_tool"))
        .await
        .expect("call tool");
    assert_eq!(result.is_error, Some(true), "{:?}", result);

    // Gracefully close the client first (closes the transport and waits
    // for the child to exit): joining the stub thread while the runtime is
    // blocked would deadlock the teardown.
    let _ = client.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = _stub.join();
}

// ---------------------------------------------------------------------------
// Result contracts end to end (tasks 6.2-6.4, 8.1; spec scenarios).
// ---------------------------------------------------------------------------

#[test]
fn submitted_value_is_returned_as_luau_value() {
    let dir = tmpdir("happy");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{
    type = "object",
    properties = {{ verdict = {{ type = "string" }}, score = {{ type = "integer" }} }},
    required = {{ "verdict" }}
}} }})
local r = s:prompt("review this")
assert(r.stop_reason == "end_turn", r.stop_reason)
assert(r.result ~= nil, "result must be submitted")
assert(r.result.verdict == "approve", r.result.verdict)
assert(r.result.score == 8, tostring(r.result.score))
assert(type(r.result) == "table")
s:close()
"#,
            mock = mock_agent(&[("MOCK_SUBMIT", r#"{"verdict":"approve","score":8}"#)])
        ),
    );
}

#[test]
fn non_object_root_schema_returns_scalar_result() {
    // Any root schema shape: an enum of strings submits as a plain string.
    let dir = tmpdir("enum");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{ type = "string", enum = {{ "ship", "block" }} }} }})
local r = s:prompt("decide")
assert(r.result == "ship", tostring(r.result))
s:close()
"#,
            mock = mock_agent(&[("MOCK_SUBMIT", r#""ship""#)])
        ),
    );
}

#[test]
fn invalid_then_valid_submission_proves_in_turn_retry() {
    let dir = tmpdir("retry");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{
    type = "object",
    properties = {{ verdict = {{ type = "string" }} }},
    required = {{ "verdict" }}
}} }})
local r = s:prompt("review this")
-- The mock submitted an invalid value twice (each a tool error naming the
-- missing property), then the corrected value: the outcome carries the
-- corrected value.
assert(r.result ~= nil and r.result.verdict == "approve", tostring(r.result))
s:close()
"#,
            mock = mock_agent(&[
                ("MOCK_SUBMIT_BAD", "2"),
                // The violation text must name the missing required property
                // end to end (message quality is the retry UX).
                ("MOCK_SUBMIT_BAD_NEEDLE", "verdict"),
                ("MOCK_SUBMIT", r#"{"verdict":"approve"}"#),
            ])
        ),
    );
}

#[test]
fn last_submission_wins_within_a_turn() {
    let dir = tmpdir("last-wins");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{
    type = "object", properties = {{ n = {{ type = "integer" }} }}, required = {{ "n" }}
}} }})
local r = s:prompt("count")
assert(r.result.n == 2, tostring(r.result and r.result.n))
s:close()
"#,
            mock = mock_agent(&[("MOCK_SUBMIT", r#"{"n":1}|{"n":2}"#)])
        ),
    );
}

#[test]
fn fresh_slot_per_turn() {
    let dir = tmpdir("fresh-slot");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{
    type = "object", properties = {{ n = {{ type = "integer" }} }}, required = {{ "n" }}
}} }})
local first = s:prompt("one")
assert(first.result ~= nil and first.result.n == 1, tostring(first.result))
-- Turn 2 ends without submitting: it must not observe turn 1's value.
local second = s:prompt("two")
assert(second.result == nil, tostring(second.result))
s:close()
"#,
            mock = mock_agent(&[("MOCK_SUBMIT", r#"{"n":1}"#), ("MOCK_SUBMIT_ONCE", "1")])
        ),
    );
}

#[test]
fn cancelled_turn_discards_its_submission() {
    let dir = tmpdir("cancel-discard");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{
    type = "object", properties = {{ n = {{ type = "integer" }} }}, required = {{ "n" }}
}} }})
local work = ponos.spawn(function() return s:prompt("slow") end)
ponos.sleep(300)
s:cancel()
local r = work:await()
assert(r.stop_reason == "cancelled", r.stop_reason)
assert(r.result == nil, "cancelled turn must discard its submission")
s:close()
"#,
            // The mock submits before entering its hang wait, so the value
            // lands in the slot and is then discarded by the cancel.
            mock = mock_agent(&[("MOCK_HANG", "1"), ("MOCK_SUBMIT", r#"{"n":1}"#)])
        ),
    );
}

#[test]
fn no_submit_turn_yields_nil_result() {
    let dir = tmpdir("no-submit");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{ type = "object" }} }})
local r = s:prompt("hello")
-- The mock echoes the (augmented) prompt text.
assert(r.text:sub(1, 5) == "hello", r.text)
assert(r.stop_reason == "end_turn")
assert(r.result == nil, "no submission must yield nil")
s:close()
"#,
            mock = mock_agent(&[])
        ),
    );
}

#[test]
fn agent_ignoring_mcp_degrades_to_nil_with_one_lifecycle_line() {
    // Task 6.4 / spec "Graceful degradation": turn completes, result is
    // nil, and exactly one lifecycle log line notes the missing typed
    // results.
    let dir = tmpdir("no-mcp");
    let (stdout, stderr, ok) = run_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{ type = "object" }} }})
local r = s:prompt("hello")
assert(r.result == nil, tostring(r.result))
assert(r.stop_reason == "end_turn", r.stop_reason)
s:close()
"#,
            mock = mock_agent(&[("MOCK_NO_MCP", "1")])
        ),
        true,
    );
    assert!(ok, "stdout:\n{stdout}\nstderr:\n{stderr}");
    let count = stdout
        .lines()
        .filter(|l| l.contains("without typed results"))
        .count();
    assert_eq!(count, 1, "exactly one lifecycle line:\n{stdout}");
}

#[test]
fn unspawnable_injected_bridge_does_not_hang_the_turn() {
    // Spec "Graceful degradation / No hang on missing bridge": the agent
    // cannot spawn the injected server (sandboxed away from the binary —
    // MOCK_MCP_UNSPAWNABLE sabotages the spawn). The turn must still
    // reach completion with result nil and one lifecycle line, even
    // though the mock would have submitted had the bridge existed.
    let dir = tmpdir("no-bridge");
    let (stdout, stderr, ok) = run_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{
    type = "object",
    properties = {{ verdict = {{ type = "string" }} }},
    required = {{ "verdict" }}
}} }})
local r = s:prompt("hello")
assert(r.stop_reason == "end_turn", r.stop_reason)
assert(r.result == nil, tostring(r.result))
s:close()
"#,
            mock = mock_agent(&[
                ("MOCK_MCP_UNSPAWNABLE", "ponos"),
                ("MOCK_SUBMIT", r#"{"verdict":"approve"}"#),
            ])
        ),
        true,
    );
    assert!(ok, "stdout:\n{stdout}\nstderr:\n{stderr}");
    let count = stdout
        .lines()
        .filter(|l| l.contains("without typed results"))
        .count();
    assert_eq!(count, 1, "exactly one lifecycle line:\n{stdout}");
}

#[test]
fn prompt_on_result_session_carries_submit_instruction() {
    // Task 5.3: the augmented prompt ends with the fixed sentence (the
    // mock echoes the prompt text it received).
    let dir = tmpdir("augment");
    let sentence = "When your work is complete, call the `mcp__ponos__result_submit` tool with your final result as the `value` argument; if the tool reports schema violations, fix the value and call it again.";
    let sentence_lit = format!("{sentence:?}");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{ type = "object" }} }})
local r = s:prompt("hello")
local suffix = {sentence}
assert(r.text:sub(-#suffix) == suffix, r.text)
-- Plain sessions are unaugmented.
s:close()
local plain = ponos.agent({mock})
local ps = plain:session()
local pr = ps:prompt("hello")
assert(pr.text == "hello", pr.text)
ps:close()
"#,
            mock = mock_agent(&[("MOCK_NO_MCP", "1")]),
            sentence = sentence_lit,
        ),
    );
}

#[test]
fn result_session_injects_ponos_server_alongside_user_servers() {
    // Task 5.2: `session/new` receives the user's servers plus the
    // injected `ponos` bridge entry.
    let dir = tmpdir("inject");
    // The JSON parse happens in the test (Luau has no json lib).
    let (stdout, _, ok) = run_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{
    result = {{ type = "object" }},
    mcp_servers = {{ {{
        type = "stdio", name = "ctx", command = "/bin/true",
        args = {{ "--flag" }}, env = {{ {{ name = "K", value = "V" }} }},
    }} }},
}})
local r = s:prompt("echo")
ponos.log(r.text)
s:close()
"#,
            mock = mock_agent(&[("MOCK_NO_MCP", "1"), ("MOCK_ECHO_MCP", "1")])
        ),
        false,
    );
    assert!(ok, "{stdout}");
    let json_line = stdout
        .lines()
        .find(|l| l.starts_with("[ponos] ["))
        .expect("mcpServers JSON in output");
    let servers: serde_json::Value =
        serde_json::from_str(json_line.trim_start_matches("[ponos] ")).expect("valid JSON");
    let arr = servers.as_array().expect("array");
    assert_eq!(arr.len(), 2, "user server + injected server: {servers}");
    assert_eq!(arr[0]["name"], "ctx");
    assert_eq!(arr[0]["command"], "/bin/true");
    let injected = &arr[1];
    assert_eq!(injected["name"], "ponos");
    assert!(
        injected["command"]
            .as_str()
            .is_some_and(|c| c.contains("ponos")),
        "injected command is the ponos binary: {injected}"
    );
    let env_names: Vec<&str> = injected["env"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(env_names.contains(&"PONOS_BRIDGE_ADDR"), "{env_names:?}");
    assert!(env_names.contains(&"PONOS_RESULT_SCHEMA"), "{env_names:?}");
    assert_eq!(injected["args"], serde_json::json!(["__bridge"]));
}

#[test]
fn injected_server_lists_single_wrapped_tool() {
    // Task 6.1 / spec "Tool appears with wrapped schema": the mock spawns
    // the real bridge and lists its tools.
    let dir = tmpdir("list-tools");
    let (stdout, stderr, ok) = run_script(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{ result = {{
    type = "object",
    properties = {{ verdict = {{ type = "string" }} }},
    required = {{ "verdict" }}
}} }})
local r = s:prompt("list")
ponos.log(r.text)
s:close()
"#,
            mock = mock_agent(&[("MOCK_MCP_LIST", "1")])
        ),
        false,
    );
    assert!(ok, "stdout:\n{stdout}\nstderr:\n{stderr}");
    let json_line = stdout
        .lines()
        .find(|l| l.starts_with("[ponos] ["))
        .expect("tool listing JSON in output");
    let listing: serde_json::Value =
        serde_json::from_str(json_line.trim_start_matches("[ponos] ")).unwrap();
    let tools = listing.as_array().expect("array");
    assert_eq!(tools.len(), 1, "{listing}");
    assert_eq!(tools[0]["name"], "result_submit");
    assert_eq!(
        tools[0]["inputSchema"]["properties"]["value"],
        serde_json::json!({
            "type": "object",
            "properties": { "verdict": { "type": "string" } },
            "required": ["verdict"]
        })
    );
    assert_eq!(
        tools[0]["inputSchema"]["required"],
        serde_json::json!(["value"])
    );
}

#[test]
fn concurrent_result_sessions_stay_independent() {
    // Task 8.1 / spec "Concurrent result sessions": two sessions with
    // different schemas run prompts concurrently; each outcome's result
    // validates against its own schema only.
    let dir = tmpdir("concurrent");
    assert_ok(
        &dir,
        &format!(
            r#"
local a = ponos.agent({mock_a})
local b = ponos.agent({mock_b})
local sa = a:session({{ result = {{
    type = "object", properties = {{ verdict = {{ type = "string" }} }}, required = {{ "verdict" }}
}} }})
local sb = b:session({{ result = {{ type = "string", enum = {{ "ship", "block" }} }} }})
local ta = ponos.spawn(function() return sa:prompt("review a") end)
local tb = ponos.spawn(function() return sb:prompt("review b") end)
local ra = ta:await()
local rb = tb:await()
assert(ra.result ~= nil and ra.result.verdict == "approve", tostring(ra.result))
assert(rb.result == "ship", tostring(rb.result))
-- Cross-checks: A's shape is not valid for B's schema and vice versa.
assert(rb.result.verdict == nil, "string result must not carry verdict")
assert(ra.result ~= "ship", "object result must not be a string")
sa:close()
sb:close()
"#,
            mock_a = mock_agent(&[("MOCK_SUBMIT", r#"{"verdict":"approve"}"#)]),
            mock_b = mock_agent(&[("MOCK_SUBMIT", r#""ship""#)]),
        ),
    );
}

// ---------------------------------------------------------------------------
// Schema declaration failures (task 4.1): run through the binary so the
// error surfaces from the real CLI path.
// ---------------------------------------------------------------------------

#[test]
fn invalid_schema_fails_at_session_call_naming_the_problem() {
    let dir = tmpdir("bad-schema");
    let script = format!(
        r#"
local agent = ponos.agent({mock})
local ok, err = pcall(function()
    return agent:session({{ result = {{ type = "objekt" }} }})
end)
assert(not ok, "session() must fail on an invalid schema")
assert(tostring(err):find("schema", 1, true), tostring(err))
ponos.log("schema error observed: " .. tostring(err))
"#,
        mock = mock_agent(&[])
    );
    let (stdout, stderr, ok) = run_script(&dir, &script, false);
    assert!(ok, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.contains("schema error observed"), "{stdout}");
}

#[test]
fn remote_ref_is_rejected_before_any_subprocess_spawns() {
    let dir = tmpdir("remote-ref");
    let script = r#"
-- The command does not exist: if the schema compiled only after spawn,
-- the error would name the command, not the schema.
local agent = ponos.agent({ command = "/nonexistent/ponos-test-agent" })
local ok, err = pcall(function()
    return agent:session({ result = { ["$ref"] = "https://example.com/schema.json" } })
end)
assert(not ok, "session() must fail on a remote $ref")
assert(tostring(err):find("remote $ref", 1, true), tostring(err))
assert(tostring(err):find("https://example.com/schema.json", 1, true), tostring(err))
"#;
    let (stdout, stderr, ok) = run_script(&dir, script, false);
    assert!(ok, "stdout:\n{stdout}\nstderr:\n{stderr}");
}

#[test]
fn valid_schema_creates_a_session_without_error() {
    let dir = tmpdir("valid-schema");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session({{
    result = {{
        type = "object",
        properties = {{ verdict = {{ type = "string" }} }},
        required = {{ "verdict" }}
    }},
}})
assert(s ~= nil)
s:close()
"#,
            mock = mock_agent(&[("MOCK_NO_MCP", "1")])
        ),
    );
}

// ---------------------------------------------------------------------------
// Permission posture (task 2.1): the mock asserts the selection/response
// itself; these tests prove the turn completes under each offer shape.
// ---------------------------------------------------------------------------

#[test]
fn permission_allow_always_is_selected() {
    let dir = tmpdir("perm-always");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session()
local r = s:prompt("work")
assert(r.stop_reason == "end_turn", r.stop_reason)
s:close()
"#,
            mock = mock_agent(&[("MOCK_PERMISSION", "always")])
        ),
    );
}

#[test]
fn permission_allow_once_fallback_is_selected() {
    let dir = tmpdir("perm-once");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session()
local r = s:prompt("work")
assert(r.stop_reason == "end_turn", r.stop_reason)
s:close()
"#,
            mock = mock_agent(&[("MOCK_PERMISSION", "once")])
        ),
    );
}

#[test]
fn permission_reject_only_offer_gets_method_not_found() {
    let dir = tmpdir("perm-reject");
    assert_ok(
        &dir,
        &format!(
            r#"
local agent = ponos.agent({mock})
local s = agent:session()
local r = s:prompt("work")
assert(r.stop_reason == "end_turn", r.stop_reason)
s:close()
"#,
            mock = mock_agent(&[("MOCK_PERMISSION", "reject")])
        ),
    );
}
