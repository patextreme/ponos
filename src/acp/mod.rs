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
//! Sessions with a typed result contract (`agent:session({ result = … })`)
//! additionally bind a per-session Unix-domain result channel and offer
//! the agent the `ponos __bridge` MCP server in `session/new`; accepted
//! submissions land in the in-flight turn's slot and ride out on
//! `TurnOutcome::result`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    BooleanConfigOptionCapabilities, CancelNotification, ClientCapabilities,
    ClientSessionCapabilities, ContentBlock, ContentChunk, EnvVariable, InitializeRequest,
    McpServer, McpServerStdio, NewSessionRequest, PermissionOption, PermissionOptionKind,
    PromptRequest, PromptResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionConfigId, SessionConfigKind,
    SessionConfigOption, SessionConfigOptionValue, SessionConfigOptionsCapabilities,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, StopReason, TextContent,
    Usage,
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
    SetConfig {
        id: String,
        value: SessionConfigOptionValue,
        resp: oneshot::Sender<Result<(), String>>,
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
    /// Per-session tool call display state. Deliberately outside
    /// `begin_turn`/`settle_turn`: entries live for the session lifetime
    /// so a repeat terminal status for an old id still dedups.
    tools: ToolFold,
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

/// Display state for one tool call (keyed by call id).
#[derive(Default)]
struct ToolCallDisplay {
    /// Title learned from the call's `tool_call` announcement; `None`
    /// until one arrives — updates for ids that were never announced fall
    /// back to the raw call id.
    title: Option<String>,
    /// Duration anchor: the `in_progress` transition once one has
    /// rendered, otherwise the call's first observation.
    first_activity: Option<Instant>,
    /// Last status a line was rendered for; a transition that repeats it
    /// renders nothing.
    last_rendered: Option<String>,
}

/// Tool-line policy for one session: which tool updates deserve a line.
/// Kept here (where the update stream arrives) rather than in the
/// renderer, which stays a dumb sink.
#[derive(Default)]
struct ToolFold {
    calls: HashMap<String, ToolCallDisplay>,
}

impl ToolFold {
    /// Fold a `tool_call` announcement. `pending` seeds the map only; an
    /// announcement already `in_progress` renders the start line; an
    /// announcement already terminal renders the terminal line, duration
    /// measured from first observation.
    fn announce(&mut self, id: &str, title: &str, status: &str, now: Instant) -> Option<String> {
        match self.calls.get_mut(id) {
            Some(entry) => entry.title = Some(title.to_string()),
            None => {
                self.calls.insert(
                    id.to_string(),
                    ToolCallDisplay {
                        title: Some(title.to_string()),
                        first_activity: Some(now),
                        last_rendered: None,
                    },
                );
            }
        }
        self.transition(id, status, now)
    }

    /// Fold a `tool_call_update` carrying a status. An id that was never
    /// announced is seeded titleless (raw-id fallback for its lines).
    fn update_status(&mut self, id: &str, status: &str, now: Instant) -> Option<String> {
        self.calls
            .entry(id.to_string())
            .or_insert_with(|| ToolCallDisplay {
                first_activity: Some(now),
                ..ToolCallDisplay::default()
            });
        self.transition(id, status, now)
    }

    /// Apply the render policy to one observed status and return the
    /// fully formatted line body when a line should render.
    ///
    /// - `pending` (and unknown statuses) never render;
    /// - `in_progress` renders the bare-title start line once — repeats
    ///   are silent (the flood guard);
    /// - terminal statuses render title + status + duration, once.
    fn transition(&mut self, id: &str, status: &str, now: Instant) -> Option<String> {
        let entry = self.calls.get_mut(id).expect("entry just seeded");
        // Title via the id→title map; the raw call id is the fallback for
        // updates that preceded their announcement.
        let title = entry.title.clone().unwrap_or_else(|| id.to_string());
        match status {
            "in_progress" => {
                if entry.last_rendered.as_deref() == Some(status) {
                    return None;
                }
                entry.last_rendered = Some(status.to_string());
                // The start line is the duration anchor once it exists.
                entry.first_activity = Some(now);
                Some(format!("tool: {title}"))
            }
            "completed" | "failed" => {
                if entry.last_rendered.as_deref() == Some(status) {
                    return None;
                }
                entry.last_rendered = Some(status.to_string());
                let anchor = entry.first_activity.unwrap_or(now);
                Some(format!(
                    "tool: {title} ({status}, {})",
                    format_duration(now - anchor)
                ))
            }
            _ => None,
        }
    }
}

/// `X.Ys` under a minute, `Mm SS.Ss` above. Tenths are rounded up-front so
/// the seconds part can never display `60.0`.
fn format_duration(d: Duration) -> String {
    let tenths = (d.as_millis() + 50) / 100;
    if tenths < 600 {
        format!("{}.{}s", tenths / 10, tenths % 10)
    } else {
        format!(
            "{}m {:02}.{}s",
            tenths / 600,
            (tenths % 600) / 10,
            tenths % 10
        )
    }
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
    // Live config-option state (session/new → updates → sets), shared
    // with the driver connection and snapshotted by the handle.
    let config_options = Arc::new(Mutex::new(Vec::<SessionConfigOption>::new()));
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
    let driver_config = config_options.clone();

    let driver = tokio::spawn(async move {
        let child_guard = child;
        let stderr_task = stderr_task;

        let notif_fold = driver_fold.clone();
        let notif_renderer = driver_renderer.clone();
        let notif_label = driver_label.clone();
        let notif_config = driver_config.clone();

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
                    fold_update(
                        &notif_config,
                        &notif_fold,
                        &notif_renderer,
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
                            let spawned = conn.spawn(async move {
                                let outcome = run_turn(
                                    &conn2,
                                    &fold,
                                    &renderer,
                                    &label,
                                    &session_id,
                                    text,
                                    timeout,
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
                        SessionCmd::SetConfig { id, value, resp } => {
                            let conn2 = conn.clone();
                            let config = driver_config.clone();
                            let renderer = driver_renderer.clone();
                            let session_id = session_id.clone();
                            let label = driver_label.clone();
                            let spawned = conn.spawn(async move {
                                let result = run_set_config(
                                    &conn2,
                                    &config,
                                    &renderer,
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
    _renderer: &Arc<Renderer>,
    _label: &str,
    session_id: &agent_client_protocol::schema::v1::SessionId,
    text: String,
    timeout: Option<Duration>,
) -> Result<TurnOutcome, TurnError> {
    // Fresh slot per turn; submissions landing before this point are late.
    fold.lock().unwrap().begin_turn();

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
    config: &Arc<Mutex<Vec<SessionConfigOption>>>,
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
            let body = fold.lock().unwrap().tools.announce(
                &call.tool_call_id.0,
                &call.title,
                &status_string(&call.status),
                Instant::now(),
            );
            if let Some(body) = body {
                renderer.event(label, DisplayEvent::Tool(body));
            }
        }
        SessionUpdate::ToolCallUpdate(update) => {
            if let Some(status) = update.fields.status {
                let body = fold.lock().unwrap().tools.update_status(
                    &update.tool_call_id.0,
                    &status_string(&status),
                    Instant::now(),
                );
                if let Some(body) = body {
                    renderer.event(label, DisplayEvent::Tool(body));
                }
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
        SessionUpdate::ConfigOptionUpdate(update) => {
            // The payload carries the full option set: replace the state
            // wholesale and note what changed (no reply exists — it's a
            // notification).
            let changed = apply_config_options(config, update.config_options);
            if !changed.is_empty() {
                renderer.lifecycle(&format!(
                    "{label}: config changed: {}",
                    format_changed(&changed)
                ));
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
    renderer: &Arc<Renderer>,
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
            renderer.lifecycle(&format!("{label}: config changed: {summary}"));
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

    // -----------------------------------------------------------------------
    // Tool-line fold policy
    // -----------------------------------------------------------------------

    #[test]
    fn tool_fold_pending_seeds_only() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(
            tools.announce("c1", "Search files \"foo\"", "pending", t0),
            None
        );
        let entry = tools.calls.get("c1").expect("pending seeds the map");
        assert_eq!(entry.title.as_deref(), Some("Search files \"foo\""));
        assert!(
            entry.first_activity.is_some(),
            "first observation anchors duration"
        );
        assert!(entry.last_rendered.is_none(), "nothing rendered yet");
    }

    #[test]
    fn tool_fold_start_then_terminal_with_duration() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(tools.announce("c1", "T", "pending", t0), None);
        // Start line at the in_progress transition: bare title, no status.
        assert_eq!(
            tools.update_status("c1", "in_progress", t0 + Duration::from_millis(100)),
            Some("tool: T".to_string())
        );
        // Terminal line: status + duration measured from the start line.
        assert_eq!(
            tools.update_status("c1", "completed", t0 + Duration::from_millis(3300)),
            Some("tool: T (completed, 3.2s)".to_string())
        );
    }

    #[test]
    fn tool_fold_repeated_statuses_are_suppressed() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        tools.announce("c1", "T", "in_progress", t0);
        // Repeated in_progress (resent by flood-prone agents).
        assert_eq!(
            tools.update_status("c1", "in_progress", t0 + Duration::from_millis(50)),
            None
        );
        // Repeated terminal status.
        tools.update_status("c1", "completed", t0 + Duration::from_millis(100));
        assert_eq!(
            tools.update_status("c1", "completed", t0 + Duration::from_millis(150)),
            None
        );
    }

    #[test]
    fn tool_fold_announcement_already_in_progress_or_terminal() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(
            tools.announce("c1", "T", "in_progress", t0),
            Some("tool: T".to_string())
        );
        assert_eq!(
            tools.announce("c2", "U", "completed", t0 + Duration::from_millis(250)),
            Some("tool: U (completed, 0.0s)".to_string())
        );
    }

    #[test]
    fn tool_fold_unannounced_update_falls_back_to_raw_id() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        assert_eq!(
            tools.update_status("call_0bb9", "in_progress", t0),
            Some("tool: call_0bb9".to_string())
        );
        assert_eq!(
            tools.update_status("call_0bb9", "failed", t0 + Duration::from_millis(200)),
            Some("tool: call_0bb9 (failed, 0.2s)".to_string())
        );
        // A late announcement still teaches the map the real title.
        assert_eq!(
            tools.announce(
                "call_0bb9",
                "Real title",
                "completed",
                t0 + Duration::from_millis(400)
            ),
            Some("tool: Real title (completed, 0.4s)".to_string())
        );
    }

    #[test]
    fn tool_fold_direct_completion_measures_from_first_observation() {
        let mut tools = ToolFold::default();
        let t0 = Instant::now();
        tools.announce("c1", "T", "pending", t0);
        assert_eq!(
            tools.update_status("c1", "completed", t0 + Duration::from_millis(1200)),
            Some("tool: T (completed, 1.2s)".to_string())
        );
    }

    #[test]
    fn duration_format_shapes() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0.0s");
        assert_eq!(format_duration(Duration::from_millis(49)), "0.0s");
        assert_eq!(format_duration(Duration::from_millis(50)), "0.1s");
        assert_eq!(format_duration(Duration::from_millis(3149)), "3.1s");
        assert_eq!(format_duration(Duration::from_millis(59_949)), "59.9s");
        // The minute boundary and rounding across it.
        assert_eq!(format_duration(Duration::from_millis(59_950)), "1m 00.0s");
        assert_eq!(format_duration(Duration::from_millis(125_040)), "2m 05.0s");
    }
}
