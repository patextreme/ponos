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
//! - `MOCK_PERMISSION`  — send `session/request_permission` (expects the
//!   client to deny; proceeds with end_turn regardless)
//! - `MOCK_HANG`        — never respond to prompts unless cancelled
//! - `MOCK_STDERR`      — write this text to stderr once per prompt
//! - `MOCK_STOP_REASON` — override the stop reason (default `end_turn`)
//! - `MOCK_ENV_DUMP`    — name of an env var: each prompt's reply text is
//!   that variable's value (for env-inheritance tests)
//! - `MOCK_ECHO_CWD`    — each prompt replies with the session's `cwd`
//!   (for default-cwd tests)
//!
//! `session/cancel` marks the in-flight turn cancelled: the awaiting prompt
//! responds with `stopReason: "cancelled"`.

use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, InitializeRequest,
    InitializeResponse, NewSessionRequest, NewSessionResponse, PermissionOption,
    PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus, PromptRequest,
    PromptResponse, RequestPermissionRequest, SessionNotification, SessionUpdate, StopReason,
    TextContent, ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, Usage,
    UsageUpdate,
};
use agent_client_protocol::{Agent, ConnectionTo, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let turn = Arc::new(TurnState::default());

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
                async move |_req: NewSessionRequest,
                            responder: agent_client_protocol::Responder<NewSessionResponse>,
                            _cx| {
                    let n = session_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    *turn.session_cwd.lock().unwrap() = Some(_req.cwd.display().to_string());
                    let _ = AgentCapabilities::new();
                    responder.respond(NewSessionResponse::new(format!("mock-session-{n}")))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let turn = turn.clone();
                async move |req: PromptRequest,
                            responder: agent_client_protocol::Responder<PromptResponse>,
                            cx: ConnectionTo<agent_client_protocol::Client>| {
                    let turn = turn.clone();
                    let conn = cx.clone();
                    cx.spawn(async move {
                        if let Err(e) = run_prompt(req, responder, conn, turn).await {
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
    Ok(())
}

async fn run_prompt(
    req: PromptRequest,
    responder: agent_client_protocol::Responder<PromptResponse>,
    cx: ConnectionTo<agent_client_protocol::Client>,
    turn: Arc<TurnState>,
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

    // Optional permission round-trip: the client must deny (method not found);
    // the agent then proceeds with its own fallback (end_turn).
    if env_flag("MOCK_PERMISSION") {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tool_call = ToolCallUpdate::new(
            "tool-perm",
            ToolCallUpdateFields::new().status(ToolCallStatus::Pending),
        );
        cx.send_request(RequestPermissionRequest::new(
            session_id.clone(),
            tool_call,
            vec![PermissionOption::new(
                "allow_once",
                "Allow",
                PermissionOptionKind::AllowOnce,
            )],
        ))
        .on_receiving_result(async move |result| {
            let _ = tx.send(result);
            Ok(())
        })?;
        match rx.await {
            Ok(Ok(_)) | Err(_) => {} // denied (or transport gone): proceed
            Ok(Err(e)) => {
                let msg = format!("{e:?}");
                if msg.contains("incoming_transport_closed") {
                    // Client vanished mid-request: proceed without asserting.
                } else {
                    // Expected: unsupported-method error from a capability-less client.
                    assert!(
                        msg.contains("ethod not found") || msg.contains("-32601"),
                        "permission denial should be -32601, got: {msg}"
                    );
                }
            }
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

    let chunks: Vec<String> = match std::env::var("MOCK_ENV_DUMP") {
        Ok(var) => vec![std::env::var(&var).unwrap_or_default()],
        Err(_) if env_flag("MOCK_ECHO_CWD") => {
            vec![turn.session_cwd.lock().unwrap().clone().unwrap_or_default()]
        }
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
