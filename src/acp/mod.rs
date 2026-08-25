//! ACP client wiring: agent process spawning, the `initialize` handshake,
//! the per-session driver, run-end teardown, and typed-result injection.
//!
//! Each ponos session owns one agent subprocess. A driver task runs the
//! JSON-RPC connection: it performs the handshake, creates the ACP session,
//! then serves a command channel (`prompt` / `set_config` / `cancel` /
//! `close`). Streaming `session/update` notifications are folded into the
//! in-flight turn's accumulator and forwarded to the renderer. ponos
//! declares exactly one client capability — the non-interactive
//! `session.configOptions` — so capability-gating agents may offer
//! per-session config options; agent-to-client requests are still answered
//! automatically so turns never hang: fs/terminal/elicitation (and anything
//! else unknown) with a JSON-RPC "method not found" (-32601) error by the
//! dispatch chain, and `session/request_permission` with the headless
//! allow-all selection (prefer `AllowAlways`, else the first other allow
//! option) registered below it.
//!
//! Sessions with a typed result contract (`agent:session({ resultSchema = … })`)
//! additionally bind a per-session Unix-domain result channel and offer
//! the agent the `ponos __bridge` MCP server in `session/new`; accepted
//! submissions land in the in-flight turn's slot and ride out on
//! `TurnOutcome::result`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    BooleanConfigOptionCapabilities, CancelNotification, ClientCapabilities,
    ClientSessionCapabilities, ContentBlock, ContentChunk, EnvVariable, InitializeRequest,
    McpServer, McpServerStdio, NewSessionRequest, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigId, SessionConfigKind, SessionConfigOption,
    SessionConfigOptionValue, SessionConfigOptionsCapabilities, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, StopReason, TextContent, Usage,
};
use agent_client_protocol::{AcpAgent, ByteStreams, Client, ConnectionTo};
use tokio::sync::{mpsc, oneshot};

use crate::core::config::AgentSpec;
use crate::core::contract::ResultContract;
use crate::core::events::{PlanEntry, PlanStatus, SessionEvent};
use crate::core::ports::{BridgeConfig, EventSink, HeadlessPolicy, InteractionPolicy};
use crate::core::turn::{PeekInputs, TurnFold, status_string, submission_sink};
use crate::result_wire::{bind_result_socket, spawn_result_channel};

/// How long a timed-out prompt waits for the (cancelled) response before
/// raising the timeout error to the script anyway.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

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
    /// The turn's last agent message: the final contiguous run of streamed
    /// text, where tool-call activity ends a message run. Falls back to
    /// the previous non-empty run when the turn ends on tool activity
    /// with no trailing message; empty for cancelled and message-less
    /// turns. Intermediate messages still stream to the renderer.
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
    SetConfig {
        id: String,
        value: SessionConfigOptionValue,
        resp: oneshot::Sender<Result<(), String>>,
    },
    Cancel,
    Close,
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
    /// Live per-session config-option state (captured at `session/new`,
    /// then folded from `config_option_update` notifications and
    /// `set_config_option` responses).
    config_options: Arc<Mutex<Vec<SessionConfigOption>>>,
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

    /// Snapshot the session's live config-option state.
    pub fn config_options(&self) -> Vec<SessionConfigOption> {
        self.config_options.lock().unwrap().clone()
    }

    /// Send one `session/set_config_option`. Serialized with prompt turns
    /// on this session via the turn lock, so config changes apply strictly
    /// between turns. Fails with a string carrying the config id and the
    /// agent's error message.
    pub async fn set_config(
        &self,
        id: String,
        value: SessionConfigOptionValue,
    ) -> Result<(), String> {
        let _turn = self.turn_lock.lock().await;
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(SessionCmd::SetConfig {
                id,
                value,
                resp: tx,
            })
            .map_err(|_| "session driver exited".to_string())?;
        match rx.await {
            Ok(result) => result,
            Err(_) => Err("session driver exited".to_string()),
        }
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
    sink: Arc<dyn EventSink>,
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
    let stderr_sink = sink.clone();
    let stderr_task = tokio::spawn(async move {
        use futures::AsyncBufReadExt;
        use futures::StreamExt;
        let mut lines = futures::io::BufReader::new(stderr).lines();
        while let Some(Ok(line)) = lines.next().await {
            stderr_sink.emit(&stderr_label, SessionEvent::StderrLine { line });
        }
    });

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SessionCmd>();
    let (ready_tx, ready_rx) = oneshot::channel::<Result<(), SessionError>>();
    let (done_tx, done_rx) = tokio::sync::watch::channel(false);

    let fold = Arc::new(Mutex::new(TurnFold::with_cwd(opts.cwd.clone())));
    // Live config-option state (session/new → updates → sets), shared
    // with the driver connection and snapshotted by the handle.
    let config_options = Arc::new(Mutex::new(Vec::<SessionConfigOption>::new()));
    let label = opts.label.clone();

    // Typed result contract: bind the per-session channel and offer the
    // agent the bridge MCP server alongside its own servers. Failures to
    // set up the channel degrade (result stays nil), never fail the
    // session.
    let mut mcp_servers = opts.mcp_servers.clone();
    let mut result_channel: Option<crate::result_wire::ResultChannel> = None;
    if let Some(contract) = opts.result.clone() {
        match std::env::current_exe() {
            Ok(exe) => match bind_result_socket().await {
                Ok((listener, path)) => {
                    let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
                    let channel = spawn_result_channel(
                        listener,
                        contract.clone(),
                        submission_sink(fold.clone()),
                        sink.clone(),
                        label.clone(),
                        cancel_tx,
                    );
                    sink.emit(
                        &label,
                        SessionEvent::Lifecycle {
                            message: format!(
                                "{label}: typed-result contract active (socket {})",
                                path.display()
                            ),
                        },
                    );
                    let bridge = BridgeConfig::ponos_bridge();
                    mcp_servers.push(McpServer::Stdio(
                        McpServerStdio::new(bridge.server_name, exe)
                            .args(vec!["__bridge".to_string()])
                            .env(vec![
                                EnvVariable::new(bridge.addr_env, path.display().to_string()),
                                EnvVariable::new(bridge.schema_env, contract.schema_json()),
                            ]),
                    ));
                    result_channel = Some(channel);
                }
                Err(e) => sink.emit(
                    &label,
                    SessionEvent::Lifecycle {
                        message: format!(
                            "{label}: typed results unavailable (cannot bind result socket: {e}); \
                             prompts will return result = nil"
                        ),
                    },
                ),
            },
            Err(e) => sink.emit(
                &label,
                SessionEvent::Lifecycle {
                    message: format!(
                        "{label}: typed results unavailable (cannot resolve ponos executable: {e}); \
                         prompts will return result = nil"
                    ),
                },
            ),
        }
    }

    let driver_label = label.clone();
    let teardown_label = label.clone();
    let driver_fold = fold.clone();
    let driver_sink = sink.clone();
    let driver_config = config_options.clone();

    // Headless permission posture for this session's agent→client
    // requests; the policy is the seam an interactive front end replaces.
    let policy: Arc<dyn InteractionPolicy> = Arc::new(HeadlessPolicy);

    let driver = tokio::spawn(async move {
        let child_guard = child;
        let stderr_task = stderr_task;

        let notif_fold = driver_fold.clone();
        let notif_sink = driver_sink.clone();
        let notif_label = driver_label.clone();
        let notif_config = driver_config.clone();
        let permission_policy = policy.clone();

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
            // The interaction policy answers permission requests (the
            // headless policy selects an allow option the agent offered —
            // see its docs). An offer with nothing selectable falls back
            // to method-not-found; every other request kind is answered
            // by the dispatch handler above.
            .on_receive_request(
                async move |req: RequestPermissionRequest,
                            responder: agent_client_protocol::Responder<
                    RequestPermissionResponse,
                >,
                            _cx| {
                    match permission_policy.select_permission(&req.options) {
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
                    fold_update(
                        &notif_config,
                        &notif_fold,
                        &notif_sink,
                        &notif_label,
                        notif.update,
                    );
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            );

        let cwd = opts.cwd.clone();
        let mcp_servers = mcp_servers.clone();

        let result: Result<(), agent_client_protocol::Error> = builder
            .connect_with(ByteStreams::new(stdin, stdout), move |conn| async move {
                // --- initialize handshake ---
                // The one client capability ponos declares:
                // `session.configOptions` (with its `boolean` sub-capability,
                // so conforming agents may offer boolean options and accept
                // boolean set values). It commits ponos to nothing
                // interactive; agent-to-client requests are still answered
                // by the deny-all dispatch below.
                let mut init = InitializeRequest::new(ProtocolVersion::V1);
                init.client_capabilities = ClientCapabilities::new().session(Some(
                    ClientSessionCapabilities::new().config_options(
                        SessionConfigOptionsCapabilities::new()
                            .boolean(BooleanConfigOptionCapabilities::new()),
                    ),
                ));
                match request(&conn, init).await {
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
                // Capture the advertised options as the session's initial
                // option state.
                if let Some(options) = new_session.config_options {
                    *driver_config.lock().unwrap() = options;
                }
                driver_sink.emit(
                    &driver_label,
                    SessionEvent::Lifecycle {
                        message: format!("{driver_label}: session ready"),
                    },
                );

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
                            let sink = driver_sink.clone();
                            let session_id = session_id.clone();
                            let label = driver_label.clone();
                            let spawned = conn.spawn(async move {
                                let outcome = run_turn(
                                    &conn2,
                                    &fold,
                                    &sink,
                                    &label,
                                    &session_id,
                                    text,
                                    timeout,
                                )
                                .await;
                                let _ = resp.send(outcome);
                                sink.emit(&label, SessionEvent::TurnEnd);
                                Ok(())
                            });
                            // If queueing failed the closure (and `resp` with it)
                            // was dropped: the awaiting prompt observes a closed
                            // channel and raises `TurnError::Closed`.
                            if let Err(e) = spawned {
                                tracing::warn!(%e, "failed to queue prompt task");
                            }
                        }
                        SessionCmd::SetConfig { id, value, resp } => {
                            let conn2 = conn.clone();
                            let config = driver_config.clone();
                            let sink = driver_sink.clone();
                            let session_id = session_id.clone();
                            let label = driver_label.clone();
                            let spawned = conn.spawn(async move {
                                let result = run_set_config(
                                    &conn2,
                                    &config,
                                    &sink,
                                    &label,
                                    &session_id,
                                    id,
                                    value,
                                )
                                .await;
                                let _ = resp.send(result);
                                Ok(())
                            });
                            // If queueing failed the closure (and `resp` with
                            // it) was dropped: the awaiting setConfig call
                            // observes a closed channel and errors.
                            if let Err(e) = spawned {
                                tracing::warn!(%e, "failed to queue set-config task");
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
                sink.emit(
                    &teardown_label,
                    SessionEvent::Lifecycle {
                        message: format!(
                            "{teardown_label}: session ended without typed results \
                             (agent never submitted through the result tool)"
                        ),
                    },
                );
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
            config_options,
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
    sink: &Arc<dyn EventSink>,
    label: &str,
    session_id: &agent_client_protocol::schema::v1::SessionId,
    text: String,
    timeout: Option<Duration>,
) -> Result<TurnOutcome, TurnError> {
    // Fresh slot per turn; submissions landing before this point are late.
    fold.lock().unwrap().begin_turn();

    // Exactly one prompt line per turn, at send time, attributed to this
    // session (render-logging "Prompt turns render a prompt line"). The
    // sink gates it on `--quiet` like every rendered line.
    sink.emit(label, SessionEvent::Prompt { text: text.clone() });

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
            // Cancelled / timed out / failed: the turn's text and any
            // submission it had gathered are discarded (settle drains both
            // so nothing leaks into the next turn on this session).
            let _ = fold.lock().unwrap().settle_turn(true);
            return Err(e);
        }
    };
    let stop_reason = stop_reason_string(&resp.stop_reason);
    // A cancelled turn's partial text is as unreliable as its discarded
    // submission; any other completion settles normally.
    let (text, result) = fold.lock().unwrap().settle_turn(stop_reason == "cancelled");
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
    config: &Arc<Mutex<Vec<SessionConfigOption>>>,
    fold: &Arc<Mutex<TurnFold>>,
    sink: &Arc<dyn EventSink>,
    label: &str,
    update: SessionUpdate,
) {
    match update {
        SessionUpdate::AgentMessageChunk(ContentChunk {
            content: ContentBlock::Text(t),
            ..
        }) => {
            let mut fold = fold.lock().unwrap();
            let message_break = fold.push_text(&t.text);
            drop(fold);
            sink.emit(
                label,
                SessionEvent::TextDelta {
                    delta: t.text,
                    message_break,
                },
            );
        }
        SessionUpdate::AgentMessageChunk(_) => {}
        SessionUpdate::ToolCall(call) => {
            let line = {
                let mut fold = fold.lock().unwrap();
                fold.break_message();
                let inputs = PeekInputs {
                    kind: Some(&call.kind),
                    locations: Some(&call.locations),
                    raw_input: call.raw_input.as_ref(),
                };
                fold.tools.announce(
                    &call.tool_call_id.0,
                    &call.title,
                    &status_string(&call.status),
                    &inputs,
                    Instant::now(),
                )
            };
            if let Some(line) = line {
                sink.emit(label, SessionEvent::ToolLine(line));
            }
        }
        SessionUpdate::ToolCallUpdate(update) => {
            let line = {
                let mut fold = fold.lock().unwrap();
                fold.break_message();
                fold.tools
                    .update(&update.tool_call_id.0, &update.fields, Instant::now())
            };
            if let Some(line) = line {
                sink.emit(label, SessionEvent::ToolLine(line));
            }
        }
        SessionUpdate::Plan(plan) => {
            let entries: Vec<PlanEntry> = plan
                .entries
                .iter()
                .map(|e| PlanEntry {
                    status: plan_status(&e.status),
                    content: e.content.clone(),
                })
                .collect();
            sink.emit(label, SessionEvent::Plan { entries });
        }
        SessionUpdate::UsageUpdate(u) => {
            sink.emit(
                label,
                SessionEvent::Usage {
                    used: u.used,
                    size: u.size,
                },
            );
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            // The payload carries the full option set: replace the state
            // wholesale and note what changed (no reply exists — it's a
            // notification).
            let changed = apply_config_options(config, update.config_options);
            if !changed.is_empty() {
                sink.emit(
                    label,
                    SessionEvent::Lifecycle {
                        message: format!("{label}: config changed: {}", format_changed(&changed)),
                    },
                );
            }
        }
        // User message echo, thoughts, and unstable updates are not rendered in v1.
        _ => {}
    }
}

/// Send one `session/set_config_option` and fold the response into the
/// session's option state. The caller holds the session's turn lock, so
/// this runs strictly between turns.
async fn run_set_config(
    conn: &ConnectionTo<agent_client_protocol::Agent>,
    config: &Arc<Mutex<Vec<SessionConfigOption>>>,
    sink: &Arc<dyn EventSink>,
    label: &str,
    session_id: &agent_client_protocol::schema::v1::SessionId,
    id: String,
    value: SessionConfigOptionValue,
) -> Result<(), String> {
    let req = SetSessionConfigOptionRequest::new(
        session_id.clone(),
        SessionConfigId::new(id.clone()),
        value.clone(),
    );
    match request(conn, req).await {
        Ok(resp) => {
            let changed = apply_config_options(config, resp.config_options);
            // One lifecycle line per successful set, naming each changed
            // option id and its new value (falls back to the requested
            // pair when the agent reports no diff).
            let summary = if changed.is_empty() {
                format!("{id}={}", option_value_display(&value))
            } else {
                format_changed(&changed)
            };
            sink.emit(
                label,
                SessionEvent::Lifecycle {
                    message: format!("{label}: config changed: {summary}"),
                },
            );
            Ok(())
        }
        Err(e) => Err(format!("setConfig(\"{id}\") failed: {e}")),
    }
}

/// Replace the session's option state wholesale and return the
/// `(id, new value)` pairs that differ from the previous state. An update
/// arriving with no prior option state reports every advertised option
/// as changed.
fn apply_config_options(
    config: &Arc<Mutex<Vec<SessionConfigOption>>>,
    new_options: Vec<SessionConfigOption>,
) -> Vec<(String, String)> {
    let mut options = config.lock().unwrap();
    let previous: std::collections::HashMap<String, String> = options
        .iter()
        .filter_map(|o| Some((o.id.0.to_string(), current_value_string(o)?)))
        .collect();
    let changed = new_options
        .iter()
        .filter_map(|o| {
            let id = o.id.0.to_string();
            let value = current_value_string(o)?;
            (previous.get(&id).map(String::as_str) != Some(value.as_str())).then_some((id, value))
        })
        .collect();
    *options = new_options;
    changed
}

/// Display string for an option's current value (select value id or
/// boolean); `None` for unknown option kinds.
fn current_value_string(opt: &SessionConfigOption) -> Option<String> {
    match &opt.kind {
        SessionConfigKind::Select(s) => Some(s.current_value.0.to_string()),
        SessionConfigKind::Boolean(b) => Some(b.current_value.to_string()),
        _ => None,
    }
}

/// Display string for a `set_config_option` value.
fn option_value_display(value: &SessionConfigOptionValue) -> String {
    if let Some(id) = value.as_value_id() {
        id.0.to_string()
    } else if let Some(b) = value.as_bool() {
        b.to_string()
    } else {
        String::new()
    }
}

/// `id=value, id=value` summary of a changed-options list.
fn format_changed(changed: &[(String, String)]) -> String {
    changed
        .iter()
        .map(|(id, value)| format!("{id}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Map a protocol plan status to the protocol-agnostic event status.
fn plan_status(status: &agent_client_protocol::schema::v1::PlanEntryStatus) -> PlanStatus {
    use agent_client_protocol::schema::v1::PlanEntryStatus::*;
    match status {
        Pending => PlanStatus::Pending,
        InProgress => PlanStatus::InProgress,
        Completed => PlanStatus::Completed,
        _ => PlanStatus::Other,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_config_options_diff() {
        use agent_client_protocol::schema::v1::{
            SessionConfigSelectOption, SessionConfigSelectOptions,
        };

        let config = Arc::new(Mutex::new(Vec::<SessionConfigOption>::new()));
        let choices = vec![
            SessionConfigSelectOption::new("opus", "Opus"),
            SessionConfigSelectOption::new("haiku", "Haiku"),
        ];
        let options = vec![
            SessionConfigOption::select(
                "model",
                "Model",
                "opus",
                SessionConfigSelectOptions::Ungrouped(choices.clone()),
            ),
            SessionConfigOption::boolean("fast", "Fast mode", false),
        ];

        // No prior state: every advertised option counts as changed.
        let changed = apply_config_options(&config, options.clone());
        assert_eq!(
            changed,
            vec![
                ("model".to_string(), "opus".to_string()),
                ("fast".to_string(), "false".to_string()),
            ]
        );

        // Identical state: nothing changed.
        let changed = apply_config_options(&config, options.clone());
        assert!(changed.is_empty(), "{changed:?}");

        // New value (and a previously unseen id): both reported.
        let mut updated = options.clone();
        updated[0] = SessionConfigOption::select(
            "model",
            "Model",
            "haiku",
            SessionConfigSelectOptions::Ungrouped(choices),
        );
        updated.push(SessionConfigOption::boolean("effort", "Effort", true));
        let changed = apply_config_options(&config, updated);
        assert_eq!(
            changed,
            vec![
                ("model".to_string(), "haiku".to_string()),
                ("effort".to_string(), "true".to_string()),
            ]
        );
    }
}
