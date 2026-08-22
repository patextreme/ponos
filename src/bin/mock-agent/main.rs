//! mock-agent: a scriptable ACP agent used by ponos's integration tests.
//!
//! Speaks the agent side of ACP v1 over stdio. Behavior is driven by
//! environment variables so tests can pick a scenario per session:
//!
//! - `MOCK_CHUNKS`      — `|`-separated chunks streamed per prompt
//!   (default: echo the prompt text as one chunk)
//! - `MOCK_DELAY_MS`    — delay between streamed chunks (default 0)
//! - `MOCK_USAGE`       — comma list `used,size,in,out,cache_read,cache_write`:
//!   emits a `usage_update` during the turn and carries
//!   token usage on the prompt response
//! - `MOCK_TOOL`        — emit a `tool_call` update (pending → completed)
//! - `MOCK_PLAN`        — emit a plan update
//! - `MOCK_PERMISSION`  — send `session/request_permission`; `once`/`1`
//!   offers only `AllowOnce`, `always` offers `AllowOnce` + `AllowAlways`
//!   (asserts the client selects `allow_always`), `reject` offers only
//!   reject options (asserts the client's unsupported-method error)
//! - `MOCK_HANG`        — never respond to prompts unless cancelled
//! - `MOCK_STDERR`      — write this text to stderr once per prompt
//! - `MOCK_STOP_REASON` — override the stop reason (default `end_turn`)
//! - `MOCK_ENV_DUMP`    — name of an env var: each prompt's reply text is
//!   that variable's value (for env-inheritance tests)
//! - `MOCK_ECHO_CWD`    — each prompt replies with the session's `cwd`
//!   (for default-cwd tests)
//! - `MOCK_ECHO_MCP`    — each prompt replies with the JSON of the
//!   `mcpServers` config received at `session/new`
//! - `MOCK_MCP_LIST`    — each prompt replies with a JSON listing of the
//!   tools offered by the injected `ponos` server
//! - `MOCK_NO_MCP`      — ignore the session's `mcpServers` entirely
//!   (spec-legal degradation of suggested servers)
//! - `MOCK_MCP_UNSPAWNABLE` — server name whose spawn is sabotaged with a
//!   nonexistent command (simulates an agent sandboxed away from the
//!   binary — the degrade path, turn must still complete)
//! - `MOCK_SUBMIT`      — `|`-separated JSON values; each prompt calls the
//!   `result_submit` tool of the `ponos` server with each value in order
//!   (last one wins in ponos)
//! - `MOCK_SUBMIT_ONCE` — with `MOCK_SUBMIT`, submit only on the first
//!   prompt (fresh-slot-per-turn tests)
//! - `MOCK_SUBMIT_BAD`  — number of invalid submissions (value from
//!   `MOCK_SUBMIT_BAD_VALUE`, default `{}`) to send first, asserting each
//!   returns a tool error naming violations, before the `MOCK_SUBMIT`
//!   values (in-turn retry proof); with `MOCK_SUBMIT_BAD_NEEDLE` each
//!   violation text must also contain that substring
//!
//! The mock is also an MCP client (rmcp): stdio servers suggested in
//! `session/new { mcpServers }` are spawned, handshaked, and torn down
//! with the session. Children are shut down gracefully when the ACP
//! connection ends.
//!
//! `session/cancel` marks the in-flight turn cancelled: the awaiting prompt
//! responds with `stopReason: "cancelled"`.

use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, InitializeRequest,
    InitializeResponse, NewSessionRequest, NewSessionResponse, PermissionOption,
    PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus, PromptRequest,
    PromptResponse, RequestPermissionRequest, SelectedPermissionOutcome, SessionNotification,
    SessionUpdate, StopReason, TextContent, ToolCall, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, Usage, UsageUpdate,
};
use agent_client_protocol::{Agent, ConnectionTo, Stdio};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::transport::TokioChildProcess;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::Notify;

/// Bound on one MCP server handshake so a wedged child cannot hang a test.
const MCP_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy)]
struct MockUsage {
    used: u64,
    size: u64,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
}

impl MockUsage {
    fn from_env() -> Option<Self> {
        let raw = std::env::var("MOCK_USAGE").ok()?;
        let parts: Vec<u64> = raw
            .split(',')
            .map(|p| p.trim().parse::<u64>().unwrap_or(0))
            .collect();
        let get = |i: usize| parts.get(i).copied().unwrap_or(0);
        Some(Self {
            used: get(0),
            size: get(1),
            input: get(2),
            output: get(3),
            cache_read: get(4),
            cache_write: get(5),
        })
    }
}

/// `RunningService` is neither `Clone` nor shareable behind one owner, so
/// each client lives behind an `Arc<Mutex<…>>` (calls are sequential in
/// every scripted scenario anyway).
type SharedMcpClient = Arc<tokio::sync::Mutex<rmcp::service::RunningService<rmcp::RoleClient, ()>>>;

/// MCP servers spawned from the session's `mcpServers` (stdio only).
#[derive(Default)]
struct McpState {
    clients: std::sync::Mutex<Vec<(String, SharedMcpClient)>>,
    /// Raw `mcpServers` config received at session/new (MOCK_ECHO_MCP).
    servers_json: std::sync::Mutex<Option<serde_json::Value>>,
    /// Prompt counter (MOCK_SUBMIT_ONCE).
    prompts: AtomicU64,
    /// Set when a `ponos` server was offered (diagnostics).
    saw_ponos: AtomicBool,
}

impl McpState {
    fn client(&self, name: &str) -> Option<SharedMcpClient> {
        self.clients
            .lock()
            .unwrap()
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| Arc::clone(c))
    }

    async fn shutdown(&self) {
        let clients = std::mem::take(&mut *self.clients.lock().unwrap());
        for (_, client) in clients {
            let _ = client.lock().await.close().await;
        }
    }
}

#[derive(Default)]
struct TurnState {
    cancelled: AtomicBool,
    cancel_notify: Notify,
    /// `cwd` of the (single) session, for MOCK_ECHO_CWD.
    session_cwd: std::sync::Mutex<Option<String>>,
}

impl TurnState {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // notify_one stores a permit when nobody is waiting yet, so a cancel
        // that lands before (or while) a prompt registers is never lost.
        self.cancel_notify.notify_one();
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

fn env_ms(name: &str) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn stop_reason_from_env() -> StopReason {
    match std::env::var("MOCK_STOP_REASON").as_deref() {
        Ok("max_tokens") => StopReason::MaxTokens,
        Ok("max_turn_requests") => StopReason::MaxTurnRequests,
        Ok("refusal") => StopReason::Refusal,
        Ok("cancelled") => StopReason::Cancelled,
        _ => StopReason::EndTurn,
    }
}

fn prompt_text(req: &PromptRequest) -> String {
    req.prompt
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Spawn and handshake the stdio servers suggested in `session/new`.
/// Failures degrade: the server is skipped (with a stderr note), never
/// fatal — agents are free to ignore suggested servers, and so is the mock.
async fn start_mcp_servers(
    mcp: &Arc<McpState>,
    servers: &[agent_client_protocol::schema::v1::McpServer],
) {
    use agent_client_protocol::schema::v1::McpServer;
    *mcp.servers_json.lock().unwrap() = Some(serde_json::to_value(servers).unwrap_or_default());
    if env_flag("MOCK_NO_MCP") {
        return;
    }
    // Replace any servers from a previous session on this connection.
    mcp.shutdown().await;
    for server in servers {
        let McpServer::Stdio(stdio) = server else {
            eprintln!("mock-agent: skipping non-stdio MCP server");
            continue;
        };
        if stdio.name == "ponos" {
            mcp.saw_ponos.store(true, Ordering::SeqCst);
        }
        // Test hook: sabotage the spawn of one named server with a
        // nonexistent command, simulating an agent sandboxed away from
        // the binary (spec degradation path — turn must still complete).
        let unspawnable = std::env::var("MOCK_MCP_UNSPAWNABLE").ok();
        let command_path = if unspawnable.as_deref() == Some(stdio.name.as_str()) {
            std::path::PathBuf::from("/nonexistent/mock-agent-sabotage")
        } else {
            stdio.command.clone()
        };
        let mut command = tokio::process::Command::new(command_path);
        command.args(&stdio.args);
        for var in &stdio.env {
            command.env(&var.name, &var.value);
        }
        let transport = match TokioChildProcess::new(command) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("mock-agent: cannot spawn MCP server {}: {e}", stdio.name);
                continue;
            }
        };
        match tokio::time::timeout(MCP_HANDSHAKE_TIMEOUT, ().serve(transport)).await {
            Ok(Ok(client)) => mcp.clients.lock().unwrap().push((
                stdio.name.clone(),
                Arc::new(tokio::sync::Mutex::new(client)),
            )),
            Ok(Err(e)) => {
                eprintln!("mock-agent: MCP handshake with {} failed: {e}", stdio.name)
            }
            Err(_) => eprintln!("mock-agent: MCP handshake with {} timed out", stdio.name),
        }
    }
}

/// Text of a tool result (concatenated text blocks).
fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("")
}

/// Call `result_submit` on the `ponos` server with `value`.
async fn submit_result(client: &SharedMcpClient, value: &serde_json::Value) -> CallToolResult {
    let mut arguments = serde_json::Map::new();
    arguments.insert("value".to_string(), value.clone());
    let client = client.lock().await;
    client
        .call_tool(CallToolRequestParams::new("result_submit").with_arguments(arguments))
        .await
        .expect("result_submit round-trip")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let turn = Arc::new(TurnState::default());
    let mcp = Arc::new(McpState::default());

    let builder = Agent
        .builder()
        .on_receive_request(
            async |_req: InitializeRequest,
                   responder: agent_client_protocol::Responder<InitializeResponse>,
                   _cx| {
                responder.respond(InitializeResponse::new(_req.protocol_version))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let turn = turn.clone();
                let mcp = mcp.clone();
                async move |req: NewSessionRequest,
                            responder: agent_client_protocol::Responder<NewSessionResponse>,
                            _cx| {
                    let n = session_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    *turn.session_cwd.lock().unwrap() = Some(req.cwd.display().to_string());
                    start_mcp_servers(&mcp, &req.mcp_servers).await;
                    let _ = AgentCapabilities::new();
                    responder.respond(NewSessionResponse::new(format!("mock-session-{n}")))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let turn = turn.clone();
                let mcp = mcp.clone();
                async move |req: PromptRequest,
                            responder: agent_client_protocol::Responder<PromptResponse>,
                            cx: ConnectionTo<agent_client_protocol::Client>| {
                    let turn = turn.clone();
                    let mcp = mcp.clone();
                    let conn = cx.clone();
                    cx.spawn(async move {
                        if let Err(e) = run_prompt(req, responder, conn, turn, mcp).await {
                            eprintln!("mock-agent: prompt failed: {e}");
                        }
                        Ok(())
                    })?;
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let turn = turn.clone();
                async move |_notif: CancelNotification, _cx| {
                    turn.cancel();
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        );

    builder.connect_to(Stdio::new()).await?;
    // The ACP connection ended: tear the spawned MCP servers down with the
    // session before the runtime goes away.
    mcp.shutdown().await;
    Ok(())
}

async fn run_prompt(
    req: PromptRequest,
    responder: agent_client_protocol::Responder<PromptResponse>,
    cx: ConnectionTo<agent_client_protocol::Client>,
    turn: Arc<TurnState>,
    mcp: Arc<McpState>,
) -> Result<(), agent_client_protocol::Error> {
    let session_id = req.session_id.clone();
    let text = prompt_text(&req);
    let delay = Duration::from_millis(env_ms("MOCK_DELAY_MS"));

    // Each turn starts with fresh cancellation state: a `session/cancel`
    // targets in-flight work, not future prompts. (A cancel that races the
    // prompt request is still honored via the notify permit.)
    turn.cancelled.store(false, Ordering::SeqCst);

    if let Some(msg) = std::env::var("MOCK_STDERR").ok().filter(|m| !m.is_empty()) {
        eprintln!("{msg}");
    }

    // Optional permission round-trip: the client answers with an offered
    // allow option (headless allow-all) or an unsupported-method error
    // when the offer has no allow option. Which shape is offered — and
    // which selection is therefore expected — is picked by MOCK_PERMISSION.
    if let Some(mode) = std::env::var("MOCK_PERMISSION")
        .ok()
        .filter(|m| !m.is_empty())
    {
        let options = match mode.as_str() {
            "always" => vec![
                PermissionOption::new("allow_once", "Allow", PermissionOptionKind::AllowOnce),
                PermissionOption::new(
                    "allow_always",
                    "Always allow",
                    PermissionOptionKind::AllowAlways,
                ),
            ],
            "reject" => vec![
                PermissionOption::new("reject_once", "Reject", PermissionOptionKind::RejectOnce),
                PermissionOption::new(
                    "reject_always",
                    "Always reject",
                    PermissionOptionKind::RejectAlways,
                ),
            ],
            _ => vec![PermissionOption::new(
                "allow_once",
                "Allow",
                PermissionOptionKind::AllowOnce,
            )],
        };
        let expected_id = match mode.as_str() {
            "always" => "allow_always",
            "reject" => "",
            _ => "allow_once",
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tool_call = ToolCallUpdate::new(
            "tool-perm",
            ToolCallUpdateFields::new().status(ToolCallStatus::Pending),
        );
        cx.send_request(RequestPermissionRequest::new(
            session_id.clone(),
            tool_call,
            options,
        ))
        .on_receiving_result(async move |result| {
            let _ = tx.send(result);
            Ok(())
        })?;
        match rx.await {
            Ok(Ok(resp)) => {
                assert_eq!(
                    resp.outcome,
                    agent_client_protocol::schema::v1::RequestPermissionOutcome::Selected(
                        SelectedPermissionOutcome::new(expected_id)
                    ),
                    "client selected the wrong permission option for mode {mode:?}"
                );
            }
            Ok(Err(e)) => {
                let msg = format!("{e:?}");
                if msg.contains("incoming_transport_closed") {
                    // Client vanished mid-request: proceed without asserting.
                } else if mode == "reject" {
                    assert!(
                        msg.contains("ethod not found") || msg.contains("-32601"),
                        "reject-only offer should get -32601, got: {msg}"
                    );
                } else {
                    panic!("client should have allowed mode {mode:?}, got: {msg}");
                }
            }
            Err(_) => {} // transport gone: proceed
        }
    }

    if env_flag("MOCK_TOOL") {
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCall(
                ToolCall::new("tool-1", "mock_tool").status(ToolCallStatus::Pending),
            ),
        ))?;
        tokio::time::sleep(delay).await;
        if turn.cancelled.load(Ordering::SeqCst) {
            return respond_cancelled(responder);
        }
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                "tool-1",
                ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
            )),
        ))?;
    }

    if env_flag("MOCK_PLAN") {
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::Plan(Plan::new(vec![PlanEntry::new(
                "mock step",
                PlanEntryPriority::Medium,
                PlanEntryStatus::InProgress,
            )])),
        ))?;
    }

    if let Some(u) = MockUsage::from_env() {
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::UsageUpdate(UsageUpdate::new(u.used, u.size)),
        ))?;
    }

    // Typed-result submissions: exercised when the session offered a
    // `ponos` server (i.e. the script declared a result contract).
    if let Some(client) = mcp.client("ponos") {
        let n = mcp.prompts.fetch_add(1, Ordering::SeqCst) + 1;
        let should_submit = !env_flag("MOCK_SUBMIT_ONCE") || n == 1;
        let attempted = should_submit
            && (std::env::var("MOCK_SUBMIT").is_ok() || std::env::var("MOCK_SUBMIT_BAD").is_ok());
        let mut submitted_ok = false;
        if should_submit {
            if let Ok(bad_count) = std::env::var("MOCK_SUBMIT_BAD") {
                let bad_count: usize = bad_count.parse().unwrap_or(0);
                let bad_value = std::env::var("MOCK_SUBMIT_BAD_VALUE")
                    .ok()
                    .and_then(|v| serde_json::from_str(&v).ok())
                    .unwrap_or(serde_json::json!({}));
                let needle = std::env::var("MOCK_SUBMIT_BAD_NEEDLE").ok();
                for _ in 0..bad_count {
                    let result = submit_result(&client, &bad_value).await;
                    assert!(
                        result.is_error == Some(true),
                        "invalid submission should be a tool error, got: {:?}",
                        result_text(&result)
                    );
                    let violations = result_text(&result);
                    assert!(!violations.is_empty(), "violation text must not be empty");
                    if let Some(needle) = &needle {
                        assert!(
                            violations.contains(needle.as_str()),
                            "violation text must name the violation ({needle:?}), got: {violations:?}"
                        );
                    }
                }
            }
            if let Ok(values) = std::env::var("MOCK_SUBMIT") {
                for raw in values.split('|') {
                    let value: serde_json::Value =
                        serde_json::from_str(raw).expect("MOCK_SUBMIT entries must be JSON");
                    let result = submit_result(&client, &value).await;
                    assert!(
                        result.is_error != Some(true),
                        "valid submission should be accepted, got: {:?}",
                        result_text(&result)
                    );
                    submitted_ok = true;
                }
            }
        }
        // Surface the submit as a tool-call update so renderers show it.
        if attempted {
            cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::ToolCall(
                    ToolCall::new("submit-1", "mcp__ponos__result_submit").status(
                        if submitted_ok {
                            ToolCallStatus::Completed
                        } else {
                            ToolCallStatus::Failed
                        },
                    ),
                ),
            ))?;
        }
    }

    let chunks: Vec<String> = match std::env::var("MOCK_ENV_DUMP") {
        Ok(var) => vec![std::env::var(&var).unwrap_or_default()],
        Err(_) if env_flag("MOCK_ECHO_CWD") => {
            vec![turn.session_cwd.lock().unwrap().clone().unwrap_or_default()]
        }
        Err(_) if env_flag("MOCK_ECHO_MCP") => {
            let json = mcp.servers_json.lock().unwrap().clone().unwrap_or_default();
            vec![serde_json::to_string(&json).unwrap_or_default()]
        }
        Err(_) if env_flag("MOCK_MCP_LIST") => match mcp.client("ponos") {
            Some(client) => {
                let tools = client
                    .lock()
                    .await
                    .list_tools(None)
                    .await
                    .expect("tools/list on ponos server");
                let listing: Vec<serde_json::Value> = tools
                    .tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "inputSchema": serde_json::Value::Object(t.input_schema.as_ref().clone()),
                        })
                    })
                    .collect();
                vec![serde_json::to_string(&listing).unwrap_or_default()]
            }
            None => vec!["no-ponos-server".to_string()],
        },
        Err(_) => match std::env::var("MOCK_CHUNKS") {
            Ok(spec) => spec.split('|').map(|s| s.to_string()).collect(),
            Err(_) => vec![text],
        },
    };

    if env_flag("MOCK_HANG") {
        // Never complete the turn on our own: wait for a cancel.
        if !turn.cancelled.load(Ordering::SeqCst) {
            turn.cancel_notify.notified().await;
        }
        return respond_cancelled(responder);
    }

    for chunk in chunks {
        if turn.cancelled.load(Ordering::SeqCst) {
            return respond_cancelled(responder);
        }
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        if turn.cancelled.load(Ordering::SeqCst) {
            return respond_cancelled(responder);
        }
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(chunk),
            ))),
        ))?;
    }

    let mut response = PromptResponse::new(stop_reason_from_env());
    if let Some(u) = MockUsage::from_env() {
        response.usage = Some(Usage::new(
            u.input + u.output + u.cache_read + u.cache_write,
            u.input,
            u.output,
        ));
    }
    responder.respond(response)
}

fn respond_cancelled(
    responder: agent_client_protocol::Responder<PromptResponse>,
) -> Result<(), agent_client_protocol::Error> {
    responder.respond(PromptResponse::new(StopReason::Cancelled))
}
