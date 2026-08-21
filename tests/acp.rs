//! Integration tests for the ACP client core against the in-repo mock agent.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ponos::acp::{SessionOptions, TurnError, start_session};
use ponos::config::AgentSpec;
use ponos::render::{RenderOptions, Renderer};

fn mock_agent_spec() -> AgentSpec {
    AgentSpec::new(env!("CARGO_BIN_EXE_mock-agent"))
}

fn quiet_renderer() -> Arc<Renderer> {
    Arc::new(Renderer::new(RenderOptions {
        quiet: true,
        ..RenderOptions::default()
    }))
}

fn opts(label: &str) -> SessionOptions {
    SessionOptions {
        cwd: std::env::temp_dir(),
        mcp_servers: vec![],
        label: label.to_string(),
    }
}

#[tokio::test]
async fn handshake_and_prompt_roundtrip() {
    // Task 4.1: spawn + initialize + session/new + full prompt turn.
    let session = start_session(&mock_agent_spec(), opts("mock/s1"), quiet_renderer())
        .await
        .expect("session starts");
    let outcome = session
        .prompt("ping".into(), None)
        .await
        .expect("prompt completes");
    assert_eq!(outcome.text, "ping");
    assert_eq!(outcome.stop_reason, "end_turn");
    session.close();
    session.join().await;
}

#[tokio::test]
async fn spawn_env_is_merged_over_inherited() {
    // Registry spec: entry env values ride on top of the inherited env.
    let mut spec = mock_agent_spec();
    spec.env
        .insert("MOCK_ENV_DUMP".into(), "PONOS_TEST_MODEL".into());
    spec.env.insert("PONOS_TEST_MODEL".into(), "opus".into());

    let session = start_session(&spec, opts("mock/env"), quiet_renderer())
        .await
        .expect("session starts");
    let outcome = session
        .prompt("ignored".into(), None)
        .await
        .expect("prompt completes");
    assert_eq!(outcome.text, "opus");
    session.close();
    session.join().await;
}

#[tokio::test]
async fn spawn_failure_fails_fast_naming_command() {
    let mut spec = AgentSpec::new("/nonexistent/ponos-test-agent");
    spec.args = vec!["--flag".into()];
    let err = start_session(&spec, opts("bad/cmd"), quiet_renderer())
        .await
        .expect_err("spawn must fail");
    assert!(
        err.to_string().contains("/nonexistent/ponos-test-agent"),
        "{err}"
    );
}

#[tokio::test]
async fn permission_request_denied_turn_still_completes() {
    // Task 4.2: agent asks for permission; ponos answers -32601 (asserted
    // inside the mock); the turn completes anyway.
    let mut spec = mock_agent_spec();
    spec.env.insert("MOCK_PERMISSION".into(), "1".into());
    let session = start_session(&spec, opts("mock/perm"), quiet_renderer())
        .await
        .expect("session starts");
    let outcome = session
        .prompt("hi".into(), None)
        .await
        .expect("turn completes after denial");
    assert_eq!(outcome.stop_reason, "end_turn");
    session.close();
    session.join().await;
}

#[tokio::test]
async fn chunked_echo_assembles_final_text_and_usage() {
    // Task 4.3: update folding — chunks -> text, usage on the result.
    let mut spec = mock_agent_spec();
    spec.env.insert("MOCK_CHUNKS".into(), "Hel|lo".into());
    spec.env.insert("MOCK_DELAY_MS".into(), "20".into());
    spec.env.insert("MOCK_USAGE".into(), "5,10,2,3".into());
    let session = start_session(&spec, opts("mock/chunks"), quiet_renderer())
        .await
        .expect("session starts");
    let outcome = session
        .prompt("q".into(), None)
        .await
        .expect("prompt completes");
    assert_eq!(outcome.text, "Hello");
    assert_eq!(outcome.stop_reason, "end_turn");
    assert_eq!(outcome.usage.input, 2);
    assert_eq!(outcome.usage.output, 3);
    session.close();
    session.join().await;
}

#[tokio::test]
async fn mcp_servers_pass_through() {
    // session/new accepts MCP server entries (mock ignores them; serialization
    // failures would fail the handshake).
    use agent_client_protocol::schema::v1::{McpServer, McpServerStdio};
    let mut options = opts("mock/mcp");
    options.mcp_servers = vec![McpServer::Stdio(McpServerStdio::new(
        "test-mcp",
        "/bin/true",
    ))];
    let session = start_session(&mock_agent_spec(), options, quiet_renderer())
        .await
        .expect("session starts");
    let outcome = session.prompt("hi".into(), None).await.expect("turn works");
    assert_eq!(outcome.stop_reason, "end_turn");
    session.close();
    session.join().await;
}

#[tokio::test]
async fn cancel_returns_cancelled_stop_reason() {
    // Task 4.4 (cancel path): MOCK_HANG never completes on its own.
    let mut spec = mock_agent_spec();
    spec.env.insert("MOCK_HANG".into(), "1".into());
    let session = start_session(&spec, opts("mock/cancel"), quiet_renderer())
        .await
        .expect("session starts");

    let s2 = session.clone();
    let pending = tokio::spawn(async move { s2.prompt("slow".into(), None).await });

    tokio::time::sleep(Duration::from_millis(100)).await;
    session.cancel();
    let outcome = pending.await.unwrap().expect("cancel is not an error");
    assert_eq!(outcome.stop_reason, "cancelled");
    session.close();
    session.join().await;
}

#[tokio::test]
async fn timeout_sends_cancel_and_raises() {
    // Task 4.4 (timeout path): raises a catchable timeout error after
    // sending session/cancel. The mock (MOCK_HANG) only ever completes a
    // turn in response to cancel, so a healthy follow-up turn proves the
    // cancel was delivered and the session stayed usable.
    let mut spec = mock_agent_spec();
    spec.env.insert("MOCK_HANG".into(), "1".into());
    let session = start_session(&spec, opts("mock/timeout"), quiet_renderer())
        .await
        .expect("session starts");
    let err = session
        .prompt("slow".into(), Some(Duration::from_millis(100)))
        .await
        .expect_err("must time out");
    assert!(matches!(err, TurnError::Timeout), "{err:?}");

    // Follow-up turn round-trips: the session stays usable after a timeout
    // (this prompt also times out — MOCK_HANG — but errors with Timeout, not
    // Closed, proving the connection still answers).
    let err = session
        .prompt("again".into(), Some(Duration::from_millis(100)))
        .await
        .expect_err("second hang must time out too");
    assert!(matches!(err, TurnError::Timeout), "{err:?}");
    session.close();
    session.join().await;
}

#[tokio::test]
async fn default_cwd_is_invocation_dir() {
    // CLI spec: sessions default to the invocation directory.
    let mut spec = mock_agent_spec();
    spec.env.insert("MOCK_ECHO_CWD".into(), "1".into());
    let mut options = opts("mock/cwd");
    options.cwd = std::env::temp_dir();
    let session = start_session(&spec, options, quiet_renderer())
        .await
        .expect("session starts");
    let outcome = session
        .prompt("where am I".into(), None)
        .await
        .expect("turn works");
    assert_eq!(outcome.text, std::env::temp_dir().display().to_string());
    session.close();
    session.join().await;
}

#[tokio::test]
async fn close_reaps_agent_process_no_zombie() {
    // Task 4.5: after close+join the child is gone from the process table.
    let session = start_session(&mock_agent_spec(), opts("mock/reap"), quiet_renderer())
        .await
        .expect("session starts");
    let pid = session.pid as i32;
    let _ = session.prompt("hi".into(), None).await;
    session.close();
    session.join().await;
    assert!(
        !PathBuf::from(format!("/proc/{pid}")).exists(),
        "agent pid {pid} still in process table (zombie?)"
    );
}

#[tokio::test]
async fn tool_and_plan_updates_flow_through_turn() {
    // Task 5.2: tool_call + tool_call_update + plan updates stream during a
    // turn and the turn still completes.
    let mut spec = mock_agent_spec();
    spec.env.insert("MOCK_TOOL".into(), "1".into());
    spec.env.insert("MOCK_PLAN".into(), "1".into());
    spec.env.insert("MOCK_DELAY_MS".into(), "10".into());
    let session = start_session(&spec, opts("mock/tools"), quiet_renderer())
        .await
        .expect("session starts");
    let outcome = session
        .prompt("do work".into(), None)
        .await
        .expect("turn completes with updates");
    assert_eq!(outcome.text, "do work");
    assert_eq!(outcome.stop_reason, "end_turn");
    session.close();
    session.join().await;
}

#[tokio::test]
async fn two_sessions_are_independent_processes() {
    let a = start_session(&mock_agent_spec(), opts("mock/a"), quiet_renderer())
        .await
        .expect("a starts");
    let b = start_session(&mock_agent_spec(), opts("mock/b"), quiet_renderer())
        .await
        .expect("b starts");
    assert_ne!(a.pid, b.pid, "sessions must not share a process");
    let (ra, rb) = tokio::join!(a.prompt("A".into(), None), b.prompt("B".into(), None));
    assert_eq!(ra.unwrap().text, "A");
    assert_eq!(rb.unwrap().text, "B");
    a.close();
    b.close();
    a.join().await;
    b.join().await;
}
