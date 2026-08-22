//! ACP client wiring: agent process spawning, the `initialize` handshake,
//! the per-session driver, run-end teardown, and typed-result injection.
//!
//! Each ponos session owns one agent subprocess. A driver task runs the
//! JSON-RPC connection: it performs the handshake, creates the ACP session,
//! then serves a command channel (`prompt` / `cancel` / `close`). Streaming
//! `session/update` notifications are folded into the in-flight turn's
//! accumulator and forwarded to the renderer. ponos declares no client
//! capabilities; agent-to-client requests are answered automatically so
//! turns never hang: fs/terminal/elicitation (and anything else unknown)
//! with a JSON-RPC "method not found" (-32601) error by the dispatch chain,
//! and `session/request_permission` with the headless allow-all selection
//! (prefer `AllowAlways`, else the first other allow option) registered
//! below it.
//!
//! Sessions with a typed result contract (`agent:session({ result = … })`)
//! additionally bind a per-session Unix-domain result channel and offer
//! the agent the `ponos __bridge` MCP server in `session/new`; accepted
//! submissions land in the in-flight turn's slot and ride out on
//! `TurnOutcome::result`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, ContentChunk, EnvVariable, InitializeRequest, McpServer,
    McpServerStdio, NewSessionRequest, PermissionOption, PermissionOptionKind, PromptRequest,
    PromptResponse, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, StopReason, TextContent, Usage,
};
use agent_client_protocol::{AcpAgent, ByteStreams, Client, ConnectionTo};
use tokio::sync::{mpsc, oneshot};

use crate::config::AgentSpec;
use crate::render::{DisplayEvent, Renderer};
use crate::result_contract::{
    ResultContract, SubmissionSink, bind_result_socket, spawn_result_channel,
};

/// How long a timed-out prompt waits for the (cancelled) response before
/// raising the timeout error to the script anyway.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

/// Fixed sentence appended to every prompt on a session with a typed
/// result contract. The schema itself travels in the tool, never in
/// prompt text.
pub const RESULT_SUBMIT_INSTRUCTION: &str = "When your work is complete, call the `mcp__ponos__result_submit` tool with your final result as the `value` argument; if the tool reports schema violations, fix the value and call it again.";

/// Token counts reported for a turn.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageCounts {
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
}

impl UsageCounts {
    fn from_usage(u: &Usage) -> Self {
        Self {
            input: u.input_tokens,
            cache_read: u.cached_read_tokens.unwrap_or(0),
            cache_write: u.cached_write_tokens.unwrap_or(0),
            output: u.output_tokens,
        }
    }
}

/// The result of one completed prompt turn.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOutcome {
    /// Final agent message text (assembled from chunks).
    pub text: String,
    /// `end_turn` | `max_tokens` | `max_turn_requests` | `refusal` | `cancelled`.
    pub stop_reason: String,
    /// Token counts (zero when unreported).
    pub usage: UsageCounts,
    /// The turn's last accepted typed submission, converted from JSON.
    /// `None` when the session declared no contract, the turn had no
    /// accepted submission, or the turn was cancelled/timed out.
    pub result: Option<serde_json::Value>,
}

/// Why a prompt turn failed.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnError {
    /// `timeoutMs` elapsed: `session/cancel` was sent, then the error raised.
    Timeout,
    /// The agent misbehaved or the protocol exchange failed.
    Agent(String),
    /// The connection closed (agent process died or was torn down).
    Closed(String),
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TurnError::Timeout => write!(f, "prompt timed out"),
            TurnError::Agent(e) => write!(f, "agent error: {e}"),
            TurnError::Closed(e) => write!(f, "connection closed: {e}"),
        }
    }
}

/// Errors starting or closing a session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionError {
    /// The configured agent command could not be spawned (names the command).
    Spawn(String),
    /// Handshake (`initialize` / `session/new`) failed.
    Handshake(String),
    /// The driver task died before the session became ready.
    DriverDied(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Spawn(msg) => write!(f, "failed to spawn agent command {msg}"),
            SessionError::Handshake(e) => write!(f, "agent handshake failed: {e}"),
            SessionError::DriverDied(e) => write!(f, "agent connection ended unexpectedly: {e}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// Options for creating one session.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Working directory for the session (default: invocation directory).
    pub cwd: PathBuf,
    /// MCP servers passed through to the agent.
    pub mcp_servers: Vec<McpServer>,
    /// Attribution label, e.g. `claude/s1`.
    pub label: String,
    /// Typed result contract. When set, ponos injects the result-bridge
    /// MCP server into the session and appends the submit instruction to
    /// every prompt.
    pub result: Option<ResultContract>,
}

/// Commands sent from Lua-side handles to the session driver.
enum SessionCmd {
    Prompt {
        text: String,
        timeout: Option<Duration>,
        resp: oneshot::Sender<Result<TurnOutcome, TurnError>>,
    },
    Cancel,
    Close,
}

/// The in-flight turn's accumulator, folded on the connection's dispatch
/// loop (in wire order, before the response is delivered).
#[derive(Default)]
struct TurnFold {
    text: String,
    /// Whether a turn is currently in flight (submissions landing outside
    /// a turn are dropped as late).
    in_flight: bool,
    /// The turn's last accepted typed submission (last-wins).
    result: Option<serde_json::Value>,
}

impl TurnFold {
    /// A turn starts: a fresh slot, so a turn never observes the previous
    /// turn's value.
    fn begin_turn(&mut self) {
        self.in_flight = true;
        self.result = None;
    }

    /// A turn settles: returns the accepted submission, or discards it
    /// (cancelled / timed-out / failed turns keep `None`).
    fn settle_turn(&mut self, discard: bool) -> Option<serde_json::Value> {
        self.in_flight = false;
        if discard {
            self.result.take();
            None
        } else {
            self.result.take()
        }
    }
}

/// The submission sink for the result channel: accept into the in-flight
/// turn's slot (last-wins), or report a late submission (no turn in
/// flight) so the channel can drop it with a lifecycle line.
fn submission_sink(fold: Arc<Mutex<TurnFold>>) -> SubmissionSink {
    Arc::new(move |value| {
        let mut fold = fold.lock().unwrap();
        if fold.in_flight {
            fold.result = Some(value);
            true
        } else {
            false
        }
    })
}

/// A handle from the scripting side to one live agent session.
#[derive(Clone)]
pub struct SessionHandle {
    /// Attribution label (`agent/session`).
    pub label: String,
    /// OS process id of the agent subprocess (for teardown assertions).
    pub pid: u32,
    cmd_tx: mpsc::UnboundedSender<SessionCmd>,
    done_rx: tokio::sync::watch::Receiver<bool>,
    /// Serializes prompt turns on this session (cancellation does not take
    /// the lock).
    turn_lock: Arc<tokio::sync::Mutex<()>>,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("label", &self.label)
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl SessionHandle {
    /// Send one prompt turn. Turns on the same session are serialized;
    /// `session:cancel()` works while a turn is in flight.
    pub async fn prompt(
        &self,
        text: String,
        timeout: Option<Duration>,
    ) -> Result<TurnOutcome, TurnError> {
        let _turn = self.turn_lock.lock().await;
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(SessionCmd::Prompt {
                text,
                timeout,
                resp: tx,
            })
            .map_err(|_| TurnError::Closed("session driver exited".into()))?;
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(TurnError::Closed("session driver exited".into())),
        }
    }

    /// Send `session/cancel` for the in-flight turn (if any).
    pub fn cancel(&self) {
        let _ = self.cmd_tx.send(SessionCmd::Cancel);
    }

    /// Ask the driver to close the session; does not wait.
    pub fn close(&self) {
        let _ = self.cmd_tx.send(SessionCmd::Close);
    }

    /// Wait for the driver (and the reaping of the agent process) to finish.
    pub async fn join(&self) {
        let mut rx = self.done_rx.clone();
        while !*rx.borrow() {
            if rx.changed().await.is_err() {
                return; // sender dropped: treat as done
            }
        }
    }
}

/// Kill the child's whole process group (agents are commonly launched via
/// `npx`-style wrappers) and reap it so no zombie remains.
async fn kill_and_reap(mut child: async_process::Child) {
    #[cfg(unix)]
    unsafe {
        let pid = child.id() as i32;
        // The child is its own process-group leader (spawn_process sets this).
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.status().await; // reap
}

/// Start one agent subprocess and drive it until closed.
pub async fn start_session(
    spec: &AgentSpec,
    opts: SessionOptions,
    renderer: Arc<Renderer>,
) -> Result<SessionHandle, SessionError> {
    let env = spec
        .env
        .iter()
        .map(|(k, v)| agent_client_protocol::schema::v1::EnvVariable::new(k.clone(), v.clone()))
        .collect::<Vec<_>>();

    let server = McpServer::Stdio(
        McpServerStdio::new(opts.label.clone(), spec.command.clone())
            .args(spec.args.clone())
            .env(env),
    );
    let agent = AcpAgent::new(server);

    let (stdin, stdout, stderr, child) = agent
        .spawn_process()
        .map_err(|e| SessionError::Spawn(format!("`{}`: {e}", spec.command)))?;
    let pid = child.id();

    let stderr_label = opts.label.clone();
    let stderr_renderer = renderer.clone();
    let stderr_task = tokio::spawn(async move {
        use futures::AsyncBufReadExt;
        use futures::StreamExt;
        let mut lines = futures::io::BufReader::new(stderr).lines();
        while let Some(Ok(line)) = lines.next().await {
            stderr_renderer.agent_stderr(&stderr_label, &line);
        }
    });

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SessionCmd>();
    let (ready_tx, ready_rx) = oneshot::channel::<Result<(), SessionError>>();
    let (done_tx, done_rx) = tokio::sync::watch::channel(false);

    let fold = Arc::new(Mutex::new(TurnFold::default()));
    let label = opts.label.clone();

    // Typed result contract: bind the per-session channel and offer the
    // agent the bridge MCP server alongside its own servers. Failures to
    // set up the channel degrade (result stays nil), never fail the
    // session.
    let mut mcp_servers = opts.mcp_servers.clone();
    let mut result_channel: Option<crate::result_contract::ResultChannel> = None;
    if let Some(contract) = opts.result.clone() {
        match std::env::current_exe() {
            Ok(exe) => match bind_result_socket().await {
                Ok((listener, path)) => {
                    let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
                    let channel = spawn_result_channel(
                        listener,
                        contract.clone(),
                        submission_sink(fold.clone()),
                        renderer.clone(),
                        label.clone(),
                        cancel_tx,
                    );
                    renderer.lifecycle(&format!(
                        "{label}: typed-result contract active (socket {})",
                        path.display()
                    ));
                    mcp_servers.push(McpServer::Stdio(
                        McpServerStdio::new(crate::bridge::SERVER_NAME, exe)
                            .args(vec!["__bridge".to_string()])
                            .env(vec![
                                EnvVariable::new("PONOS_BRIDGE_ADDR", path.display().to_string()),
                                EnvVariable::new("PONOS_RESULT_SCHEMA", contract.schema_json()),
                            ]),
                    ));
                    result_channel = Some(channel);
                }
                Err(e) => renderer.lifecycle(&format!(
                    "{label}: typed results unavailable (cannot bind result socket: {e}); \
                     prompts will return result = nil"
                )),
            },
            Err(e) => renderer.lifecycle(&format!(
                "{label}: typed results unavailable (cannot resolve ponos executable: {e}); \
                 prompts will return result = nil"
            )),
        }
    }

    let driver_label = label.clone();
    let teardown_label = label.clone();
    let driver_fold = fold.clone();
    let driver_renderer = renderer.clone();

    let driver = tokio::spawn(async move {
        let child_guard = child;
        let stderr_task = stderr_task;

        let notif_fold = driver_fold.clone();
        let notif_renderer = driver_renderer.clone();
        let notif_label = driver_label.clone();

        let builder = Client
            .builder()
            // ponos declares no client capabilities: agent-to-client
            // requests it has no support for (fs, terminal, elicitation,
            // …) are answered promptly with a method-not-found (-32601)
            // error so turns never hang. The one exception — ponos runs
            // headless and nobody is there to be asked — is
            // `session/request_permission`, which is passed through
            // (`retry: true`) to the allow-all handler below.
            .on_receive_dispatch(
                async move |dispatch: agent_client_protocol::Dispatch, _cx| match dispatch {
                    agent_client_protocol::Dispatch::Request(msg, responder) => {
                        if msg.method() == "session/request_permission" {
                            Ok(agent_client_protocol::Handled::No {
                                message: agent_client_protocol::Dispatch::Request(msg, responder),
                                retry: true,
                            })
                        } else {
                            responder.respond_with_error(
                                agent_client_protocol::Error::method_not_found(),
                            )?;
                            Ok(agent_client_protocol::Handled::Yes)
                        }
                    }
                    other => Ok(agent_client_protocol::Handled::No {
                        message: other,
                        retry: false,
                    }),
                },
                agent_client_protocol::on_receive_dispatch!(),
            )
            // Headless allow-all posture: select an allow option the agent
            // offered — the first `AllowAlways` when present, otherwise the
            // first other allow-kind option (e.g. `AllowOnce`). An offer
            // with no allow option at all falls back to method-not-found
            // (there is nothing to select). Choosing `AllowAlways` may let
            // the agent persist an allow rule in its own settings beyond
            // the run (documented in the README).
            .on_receive_request(
                async move |req: RequestPermissionRequest,
                            responder: agent_client_protocol::Responder<
                    RequestPermissionResponse,
                >,
                            _cx| {
                    match select_allow_option(&req.options) {
                        Some(option_id) => responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                option_id,
                            )),
                        )),
                        None => responder
                            .respond_with_error(agent_client_protocol::Error::method_not_found()),
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_notification(
                async move |notif: SessionNotification, _cx| {
                    fold_update(&notif_fold, &notif_renderer, &notif_label, notif.update);
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            );

        let cwd = opts.cwd.clone();
        let mcp_servers = mcp_servers.clone();

        let result: Result<(), agent_client_protocol::Error> = builder
            .connect_with(ByteStreams::new(stdin, stdout), move |conn| async move {
                // --- initialize handshake ---
                match request(&conn, InitializeRequest::new(ProtocolVersion::V1)).await {
                    Ok(_) => {}
                    Err(e) => {
                        let _ = ready_tx.send(Err(SessionError::Handshake(e.to_string())));
                        return Ok(());
                    }
                }

                // --- session/new ---
                let mut new_session_req = NewSessionRequest::new(cwd);
                new_session_req.mcp_servers = mcp_servers;
                let new_session = match request(&conn, new_session_req).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        let _ = ready_tx.send(Err(SessionError::Handshake(e.to_string())));
                        return Ok(());
                    }
                };
                let session_id = new_session.session_id;
                driver_renderer.lifecycle(&format!("{driver_label}: session ready"));

                let _ = ready_tx.send(Ok(()));

                // --- command loop ---
                while let Some(cmd) = cmd_rx.recv().await {
                    match cmd {
                        SessionCmd::Prompt {
                            text,
                            timeout,
                            resp,
                        } => {
                            let conn2 = conn.clone();
                            let fold = fold.clone();
                            let renderer = driver_renderer.clone();
                            let session_id = session_id.clone();
                            let label = driver_label.clone();
                            let result_contract = opts.result.is_some();
                            let spawned = conn.spawn(async move {
                                let outcome = run_turn(
                                    &conn2,
                                    &fold,
                                    &renderer,
                                    &label,
                                    &session_id,
                                    text,
                                    timeout,
                                    result_contract,
                                )
                                .await;
                                let _ = resp.send(outcome);
                                renderer.flush_session(&label);
                                Ok(())
                            });
                            // If queueing failed the closure (and `resp` with it)
                            // was dropped: the awaiting prompt observes a closed
                            // channel and raises `TurnError::Closed`.
                            if let Err(e) = spawned {
                                tracing::warn!(%e, "failed to queue prompt task");
                            }
                        }
                        SessionCmd::Cancel => {
                            if let Err(e) =
                                conn.send_notification(CancelNotification::new(session_id.clone()))
                            {
                                tracing::warn!(%e, "failed to send session/cancel");
                            }
                        }
                        SessionCmd::Close => break,
                    }
                }

                Ok(())
            })
            .await;

        if let Err(e) = result {
            tracing::debug!(%e, "agent connection ended");
        }
        kill_and_reap(child_guard).await;
        // Drain the stderr pump so -vv passthrough is complete before the
        // session is reported closed.
        let _ = stderr_task.await;
        // Tear the result channel down with the session: stop accepting,
        // unlink the socket, and — if nothing was ever submitted through
        // it — note the (designed) degradation once.
        if let Some(channel) = result_channel {
            let had_results = channel.any_accepted();
            channel.close().await;
            if !had_results {
                renderer.lifecycle(&format!(
                    "{teardown_label}: session ended without typed results \
                     (agent never submitted through the result tool)"
                ));
            }
        }
        let _ = done_tx.send(true);
    });

    match ready_rx.await {
        Ok(Ok(())) => Ok(SessionHandle {
            label,
            pid,
            cmd_tx,
            done_rx,
            turn_lock: Arc::new(tokio::sync::Mutex::new(())),
        }),
        Ok(Err(e)) => {
            let _ = driver.await;
            Err(e)
        }
        Err(_) => {
            let _ = driver.await;
            Err(SessionError::DriverDied(
                "driver exited before ready".into(),
            ))
        }
    }
}

/// Send a typed request and resolve its response through a oneshot.
async fn request<Req: agent_client_protocol::JsonRpcRequest>(
    conn: &ConnectionTo<agent_client_protocol::Agent>,
    req: Req,
) -> Result<Req::Response, String>
where
    Req::Response: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    conn.send_request(req)
        .on_receiving_result(async move |result| {
            let _ = tx.send(result);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    match rx.await {
        Ok(Ok(resp)) => Ok(resp),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("connection closed before response".into()),
    }
}

/// Drive one prompt turn: send `session/prompt`, race the deadline, fold
/// streaming updates, and deliver the outcome.
#[allow(clippy::too_many_arguments)]
async fn run_turn(
    conn: &ConnectionTo<agent_client_protocol::Agent>,
    fold: &Arc<Mutex<TurnFold>>,
    _renderer: &Arc<Renderer>,
    _label: &str,
    session_id: &agent_client_protocol::schema::v1::SessionId,
    text: String,
    timeout: Option<Duration>,
    result_contract: bool,
) -> Result<TurnOutcome, TurnError> {
    // Fresh slot per turn; submissions landing before this point are late.
    fold.lock().unwrap().begin_turn();

    // Sessions with a contract append the fixed submit instruction; the
    // schema itself never enters prompt text (it lives in the tool).
    let text = if result_contract {
        format!("{text}\n\n{RESULT_SUBMIT_INSTRUCTION}")
    } else {
        text
    };

    let req = PromptRequest::new(
        session_id.clone(),
        vec![ContentBlock::Text(TextContent::new(text))],
    );

    let (tx, mut rx) = oneshot::channel();
    conn.send_request(req)
        .on_receiving_result(async move |result| {
            let _ = tx.send(result);
            Ok(())
        })
        .map_err(|e| TurnError::Agent(e.to_string()))?;

    let response: Result<PromptResponse, TurnError> = match timeout {
        None => match rx.await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(e)) => Err(TurnError::Agent(e.to_string())),
            Err(_) => Err(TurnError::Closed("agent connection ended".into())),
        },
        Some(limit) => match tokio::time::timeout(limit, &mut rx).await {
            Ok(Ok(Ok(resp))) => Ok(resp),
            Ok(Ok(Err(e))) => Err(TurnError::Agent(e.to_string())),
            Ok(Err(_)) => Err(TurnError::Closed("agent connection ended".into())),
            Err(_elapsed) => {
                // Timed out: cancel remotely, then give the agent a bounded
                // grace period to land its (cancelled) response before
                // raising the timeout error regardless.
                let _ = conn.send_notification(CancelNotification::new(session_id.clone()));
                let _ = tokio::time::timeout(CANCEL_GRACE, rx).await;
                Err(TurnError::Timeout)
            }
        },
    };

    let resp = match response {
        Ok(resp) => resp,
        Err(e) => {
            // Cancelled / timed out / failed: any submission the turn had
            // gathered is discarded.
            fold.lock().unwrap().settle_turn(true);
            return Err(e);
        }
    };
    let stop_reason = stop_reason_string(&resp.stop_reason);
    let text = std::mem::take(&mut fold.lock().unwrap().text);
    // A cancelled turn discards its submission; any other completion
    // carries the last accepted one.
    let result = fold.lock().unwrap().settle_turn(stop_reason == "cancelled");
    Ok(TurnOutcome {
        text,
        stop_reason,
        usage: resp
            .usage
            .as_ref()
            .map(UsageCounts::from_usage)
            .unwrap_or_default(),
        result,
    })
}

/// Fold one streaming update into the turn accumulator + renderer.
fn fold_update(
    fold: &Arc<Mutex<TurnFold>>,
    renderer: &Arc<Renderer>,
    label: &str,
    update: SessionUpdate,
) {
    match update {
        SessionUpdate::AgentMessageChunk(ContentChunk {
            content: ContentBlock::Text(t),
            ..
        }) => {
            fold.lock().unwrap().text.push_str(&t.text);
            renderer.event(label, DisplayEvent::Chunk(t.text));
        }
        SessionUpdate::AgentMessageChunk(_) => {}
        SessionUpdate::ToolCall(call) => {
            renderer.event(
                label,
                DisplayEvent::Tool {
                    title: call.title,
                    status: status_string(&call.status),
                },
            );
        }
        SessionUpdate::ToolCallUpdate(update) => {
            if let Some(status) = update.fields.status {
                renderer.event(
                    label,
                    DisplayEvent::Tool {
                        title: update.tool_call_id.0.to_string(),
                        status: status_string(&status),
                    },
                );
            }
        }
        SessionUpdate::Plan(plan) => {
            let entries: Vec<String> = plan
                .entries
                .iter()
                .map(|e| format!("[{}] {}", entry_status(&e.status), e.content))
                .collect();
            renderer.event(
                label,
                DisplayEvent::Plan(format!("plan: {}", entries.join(" "))),
            );
        }
        SessionUpdate::UsageUpdate(u) => {
            renderer.event(
                label,
                DisplayEvent::Usage {
                    used: u.used,
                    size: u.size,
                },
            );
        }
        // User message echo, thoughts, and unstable updates are not rendered in v1.
        _ => {}
    }
}

fn status_string(status: &agent_client_protocol::schema::v1::ToolCallStatus) -> String {
    use agent_client_protocol::schema::v1::ToolCallStatus::*;
    match status {
        Pending => "pending".into(),
        InProgress => "in_progress".into(),
        Completed => "completed".into(),
        Failed => "failed".into(),
        _ => "unknown".into(),
    }
}

fn entry_status(status: &agent_client_protocol::schema::v1::PlanEntryStatus) -> char {
    use agent_client_protocol::schema::v1::PlanEntryStatus::*;
    match status {
        Pending => ' ',
        InProgress => '>',
        Completed => 'x',
        _ => '?',
    }
}

fn stop_reason_string(reason: &StopReason) -> String {
    match reason {
        StopReason::EndTurn => "end_turn".into(),
        StopReason::MaxTokens => "max_tokens".into(),
        StopReason::MaxTurnRequests => "max_turn_requests".into(),
        StopReason::Refusal => "refusal".into(),
        StopReason::Cancelled => "cancelled".into(),
        _ => "end_turn".into(),
    }
}

/// Pick the option to answer a permission request with: the first
/// `AllowAlways` when offered, otherwise the first other allow-kind
/// option. `None` when the offer has no allow option at all.
fn select_allow_option(
    options: &[PermissionOption],
) -> Option<agent_client_protocol::schema::v1::PermissionOptionId> {
    options
        .iter()
        .find(|o| matches!(o.kind, PermissionOptionKind::AllowAlways))
        .or_else(|| {
            options
                .iter()
                .find(|o| matches!(o.kind, PermissionOptionKind::AllowOnce))
        })
        .map(|o| o.option_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{PermissionOption, PermissionOptionId};

    fn option(id: &str, kind: PermissionOptionKind) -> PermissionOption {
        PermissionOption::new(id.to_string(), "label", kind)
    }

    #[test]
    fn allow_selection_prefers_allow_always() {
        let options = vec![
            option("allow_once", PermissionOptionKind::AllowOnce),
            option("allow_always", PermissionOptionKind::AllowAlways),
        ];
        assert_eq!(
            select_allow_option(&options),
            Some(PermissionOptionId::new("allow_always"))
        );
    }

    #[test]
    fn allow_selection_falls_back_to_any_allow_kind() {
        let options = vec![
            option("reject_once", PermissionOptionKind::RejectOnce),
            option("allow_once", PermissionOptionKind::AllowOnce),
        ];
        assert_eq!(
            select_allow_option(&options),
            Some(PermissionOptionId::new("allow_once"))
        );
    }

    #[test]
    fn allow_selection_reject_only_offer_gets_method_not_found() {
        let options = vec![
            option("reject_once", PermissionOptionKind::RejectOnce),
            option("reject_always", PermissionOptionKind::RejectAlways),
        ];
        assert_eq!(select_allow_option(&options), None);
        assert_eq!(select_allow_option(&[]), None);
    }

    #[test]
    fn turn_fold_slot_lifecycle() {
        let mut fold = TurnFold::default();
        // Before any turn: submissions are late.
        assert!(!submission_sink(Arc::new(Mutex::new(TurnFold::default())))(
            serde_json::json!({"n": 1})
        ));

        // In flight: accepted, last-wins.
        fold.begin_turn();
        assert!(fold.in_flight && fold.result.is_none());
        fold.result = Some(serde_json::json!({"n": 1}));
        fold.result = Some(serde_json::json!({"n": 2}));
        assert_eq!(fold.settle_turn(false), Some(serde_json::json!({"n": 2})));
        assert!(!fold.in_flight && fold.result.is_none());

        // Fresh slot per turn: a second turn without submissions yields
        // None even though the first turn had one.
        fold.begin_turn();
        assert_eq!(fold.settle_turn(false), None);

        // Discard on cancelled/failed turns.
        fold.begin_turn();
        fold.result = Some(serde_json::json!({"n": 3}));
        assert_eq!(fold.settle_turn(true), None);
        assert!(!fold.in_flight && fold.result.is_none());
    }
}
