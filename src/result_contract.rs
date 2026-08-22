//! Typed result contracts: eager JSON Schema compilation, the per-session
//! Unix-domain-socket result channel, and the newline-JSON submit/verdict
//! protocol shared by ponos-main and the hidden `ponos __bridge`
//! subcommand.
//!
//! A session that declares `agent:session({ result = <schema> })` compiles
//! the schema eagerly (author errors fail at the author's line, and remote
//! `$ref`s are rejected so runs stay offline), binds a per-session socket,
//! and offers the agent an MCP server (the bridge) whose single
//! `result_submit` tool relays submissions over that socket. ponos-main
//! validates each submission against the compiled schema and answers with
//! the verdict — that blocking round-trip is the in-turn retry mechanism:
//! a violation is a tool error the model sees and can fix inside the same
//! turn.
//!
//! Protocol (one JSON object per line, UTF-8, `\n`-terminated):
//!
//! ```text
//! bridge  ->  ponos : {"op":"submit","value":<any JSON>}
//! ponos   ->  bridge: {"ok":true}
//!                   | {"ok":false,"errors":["...","..."]}
//! ```
//!
//! One connection may carry any number of submit/verdict exchanges. The
//! socket path doubles as the capability token: it is handed only to that
//! session's agent (inside the injected server's env) and is unguessable
//! without it. Unix-only by design (the transport is a UDS).

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;

use crate::render::Renderer;

/// Upper bound on violations relayed in one verdict (message quality, not
/// a flood).
const MAX_VIOLATIONS: usize = 10;

/// Filename prefix for per-session result sockets (`ponos-r-<32hex>.sock`).
const SOCKET_PREFIX: &str = "ponos-r-";

/// A compiled JSON Schema contract for one session's typed results.
#[derive(Clone)]
pub struct ResultContract {
    schema: serde_json::Value,
    validator: jsonschema::Validator,
}

impl std::fmt::Debug for ResultContract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResultContract")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl ResultContract {
    /// Compile a schema eagerly. Fails on invalid schemas and on any
    /// non-local `$ref` (remote references would reintroduce network
    /// access; runs must stay offline).
    pub fn compile(schema: serde_json::Value) -> Result<Self, String> {
        reject_remote_refs(&schema)?;
        let validator =
            jsonschema::validator_for(&schema).map_err(|e| format!("invalid schema: {e}"))?;
        Ok(Self { schema, validator })
    }

    /// The declared schema, as JSON.
    pub fn schema(&self) -> &serde_json::Value {
        &self.schema
    }

    /// The declared schema serialized to a JSON string (for the injected
    /// server's env).
    pub fn schema_json(&self) -> String {
        self.schema.to_string()
    }

    /// Validate a submission; `Err` carries human-readable violations
    /// ("`"score" is a required property`"-style, with instance paths).
    pub fn validate(&self, value: &serde_json::Value) -> Result<(), Vec<String>> {
        let errors: Vec<String> = self
            .validator
            .iter_errors(value)
            .take(MAX_VIOLATIONS)
            .map(|e| {
                let path = e.instance_path();
                if path.as_str().is_empty() {
                    e.to_string()
                } else {
                    format!("{e} (at {path})")
                }
            })
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Reject `$ref` values that are not local JSON pointers (`#…`) within the
/// same document. Walks the whole schema — a remote reference anywhere
/// fails the contract at the author's line.
fn reject_remote_refs(schema: &serde_json::Value) -> Result<(), String> {
    match schema {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(reference)) = map.get("$ref")
                && !reference.starts_with('#')
            {
                return Err(format!(
                    "remote $ref {reference:?} is not allowed: result schemas must be \
                     self-contained (offline runs)"
                ));
            }
            for value in map.values() {
                reject_remote_refs(value)?;
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for item in items {
                reject_remote_refs(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Where accepted submissions land: the in-flight turn's slot. The closure
/// returns `true` when the submission was accepted into a live turn, and
/// `false` when no turn was in flight (a late submission to drop).
pub type SubmissionSink = Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>;

/// One request from the bridge.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum BridgeRequest {
    Submit { value: serde_json::Value },
}

/// The verdict for one submission.
#[derive(Debug, Serialize, Deserialize)]
struct Verdict {
    ok: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

/// Directory for per-session sockets: `$XDG_RUNTIME_DIR` (Linux) or
/// `$TMPDIR` (macOS), falling back to the platform temp dir.
fn socket_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("TMPDIR").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir)
}

/// 32 hex characters: 16 random bytes from `/dev/urandom`, with a
/// per-process seeded-hash fallback.
fn random_hex32() -> String {
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let mut buf = [0u8; 16];
        if f.read_exact(&mut buf).is_ok() {
            return buf.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut a = std::collections::hash_map::RandomState::new().build_hasher();
    let mut b = std::collections::hash_map::RandomState::new().build_hasher();
    a.write_u64(n);
    a.write_u64(std::process::id() as u64);
    b.write_u64(a.finish() ^ 0x9e37_79b9_7f4a_7c15);
    b.write_u64(n);
    format!("{:016x}{:016x}", a.finish(), b.finish())
}

/// Bind a fresh per-session result socket. A stale socket file (path
/// exists, nothing listening) is unlinked and rebound; a live one means a
/// name collision and a new name is drawn.
pub async fn bind_result_socket() -> std::io::Result<(UnixListener, PathBuf)> {
    for _ in 0..8 {
        let path = socket_dir().join(format!("{SOCKET_PREFIX}{}.sock", random_hex32()));
        match bind_at(&path).await {
            Ok(listener) => return Ok((listener, path)),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                // Live socket holds the name: pick another.
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        "cannot find a free result-socket name",
    ))
}

/// Bind at an exact path, handling the stale-socket case: if the path
/// exists but nothing is listening, unlink it and bind; if a live socket
/// holds it, fail with `AddrInUse`.
async fn bind_at(path: &std::path::Path) -> std::io::Result<UnixListener> {
    match UnixListener::bind(path) {
        Ok(listener) => Ok(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            let live = tokio::time::timeout(
                std::time::Duration::from_millis(250),
                UnixStream::connect(path),
            )
            .await
            .is_ok_and(|r| r.is_ok());
            if !live {
                // Stale: no listener behind the file. Unlink and rebind.
                let _ = std::fs::remove_file(path);
                UnixListener::bind(path)
            } else {
                Err(e)
            }
        }
        Err(e) => Err(e),
    }
}

/// Guard for a running result channel: aborts the accept loop and unlinks
/// the socket path.
pub struct ResultChannel {
    task: tokio::task::JoinHandle<()>,
    path: PathBuf,
    /// Set once any submission has been accepted into a turn (drives the
    /// end-of-session degradation note).
    any_accepted: Arc<AtomicBool>,
    cancel: watch::Sender<bool>,
}

impl ResultChannel {
    /// Stop the channel: cancel the accept loop, wait (bounded) for it to
    /// finish, and unlink the socket path.
    pub async fn close(self) {
        let _ = self.cancel.send(true);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), self.task).await;
        let _ = std::fs::remove_file(&self.path);
    }

    /// Whether any submission was ever accepted into a turn.
    pub fn any_accepted(&self) -> bool {
        self.any_accepted.load(Ordering::Relaxed)
    }
}

/// Run the result channel: accept connections, validate submissions
/// against the contract, deliver accepted values to `sink`, and write
/// verdicts. Late submissions (no turn in flight) are dropped with one
/// lifecycle log line each.
pub fn spawn_result_channel(
    listener: UnixListener,
    contract: ResultContract,
    sink: SubmissionSink,
    renderer: Arc<Renderer>,
    label: String,
    cancel: watch::Sender<bool>,
) -> ResultChannel {
    let mut cancel_rx = cancel.subscribe();
    let socket_path = local_addr_path(&listener);
    let any_accepted = Arc::new(AtomicBool::new(false));
    let any_task = any_accepted.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_rx.changed() => break,
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        let contract = contract.clone();
                        let sink = sink.clone();
                        let renderer = renderer.clone();
                        let label = label.clone();
                        let any = any_task.clone();
                        tokio::spawn(async move {
                            serve_connection(stream, contract, sink, renderer, label, any).await;
                        });
                    }
                    Err(e) => {
                        tracing::warn!(%e, "result channel accept failed");
                        break;
                    }
                }
            }
        }
    });
    ResultChannel {
        task,
        path: socket_path,
        any_accepted,
        cancel,
    }
}

/// The listener's local path, read before it is moved into the task.
fn local_addr_path(listener: &UnixListener) -> PathBuf {
    listener
        .local_addr()
        .ok()
        .and_then(|a| a.as_pathname().map(PathBuf::from))
        .unwrap_or_else(|| socket_dir().join(format!("{SOCKET_PREFIX}unknown.sock")))
}

/// Serve one bridge connection: newline-JSON submit/verdict exchanges
/// until the bridge disconnects.
async fn serve_connection(
    stream: UnixStream,
    contract: ResultContract,
    sink: SubmissionSink,
    renderer: Arc<Renderer>,
    label: String,
    any_accepted: Arc<AtomicBool>,
) {
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let request: BridgeRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let verdict = Verdict {
                    ok: false,
                    errors: vec![format!("malformed submit request: {e}")],
                };
                if write_verdict(&mut write_half, &verdict).await.is_err() {
                    break;
                }
                continue;
            }
        };
        let BridgeRequest::Submit { value } = request;
        match contract.validate(&value) {
            Ok(()) => {
                if sink(value) {
                    any_accepted.store(true, Ordering::Relaxed);
                    if write_verdict(
                        &mut write_half,
                        &Verdict {
                            ok: true,
                            errors: vec![],
                        },
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                } else {
                    // No turn in flight: drop, but tell the bridge it was
                    // fine (the model already finished; a late submit must
                    // not look like a validation failure).
                    renderer.lifecycle(&format!(
                        "{label}: dropped late typed-result submission (no turn in flight)"
                    ));
                    if write_verdict(
                        &mut write_half,
                        &Verdict {
                            ok: true,
                            errors: vec![],
                        },
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
            }
            Err(errors) => {
                if write_verdict(&mut write_half, &Verdict { ok: false, errors })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

async fn write_verdict(write_half: &mut OwnedWriteHalf, verdict: &Verdict) -> std::io::Result<()> {
    let mut line =
        serde_json::to_string(verdict).map_err(|e| std::io::Error::other(e.to_string()))?;
    line.push('\n');
    write_half.write_all(line.as_bytes()).await?;
    write_half.flush().await
}

/// One relayed submission from the bridge's side of the socket: send and
/// block for the verdict.
pub async fn submit_over_socket(
    stream: &mut UnixStream,
    value: &serde_json::Value,
) -> std::io::Result<Result<(), Vec<String>>> {
    use tokio::io::AsyncReadExt;
    let mut request = serde_json::json!({ "op": "submit", "value": value }).to_string();
    request.push('\n');
    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;

    // Read one verdict line (bounded so a dead peer cannot hang the
    // bridge's tool call forever).
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = tokio::time::timeout(std::time::Duration::from_secs(120), stream.read(&mut chunk))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "verdict timeout"))??;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "result channel closed before verdict",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&buf[..nl]).into_owned();
            let verdict: Verdict = serde_json::from_str(&line)
                .map_err(|e| std::io::Error::other(format!("malformed verdict: {e}")))?;
            return Ok(if verdict.ok {
                Ok(())
            } else {
                Err(verdict.errors)
            });
        }
    }
}

/// Connect to a result channel by socket path (bridge side).
pub async fn connect(path: &std::path::Path) -> std::io::Result<UnixStream> {
    UnixStream::connect(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> ResultContract {
        ResultContract::compile(serde_json::json!({
            "type": "object",
            "properties": { "verdict": { "type": "string" }, "score": { "type": "integer" } },
            "required": ["verdict"]
        }))
        .expect("schema compiles")
    }

    #[test]
    fn compile_rejects_remote_refs() {
        let err = ResultContract::compile(serde_json::json!({
            "$ref": "https://example.com/schema.json"
        }))
        .expect_err("remote ref must fail");
        assert!(err.contains("remote $ref"), "{err}");
        assert!(err.contains("https://example.com/schema.json"), "{err}");

        let err = ResultContract::compile(serde_json::json!({
            "type": "object",
            "properties": { "nested": { "$ref": "other-schema.json" } }
        }))
        .expect_err("nested remote ref must fail");
        assert!(err.contains("other-schema.json"), "{err}");
    }

    #[test]
    fn compile_accepts_local_refs() {
        ResultContract::compile(serde_json::json!({
            "$defs": { "v": { "type": "string" } },
            "type": "object",
            "properties": { "verdict": { "$ref": "#/$defs/v" } },
            "required": ["verdict"]
        }))
        .expect("local refs are fine");
    }

    #[test]
    fn compile_rejects_invalid_schemas() {
        let err = ResultContract::compile(serde_json::json!({ "type": "objekt" }))
            .expect_err("bad type value must fail");
        assert!(!err.is_empty());
    }

    #[test]
    fn validate_names_violations_and_paths() {
        let c = contract();
        assert!(
            c.validate(&serde_json::json!({ "verdict": "approve" }))
                .is_ok()
        );
        let errors = c
            .validate(&serde_json::json!({ "score": 3 }))
            .expect_err("missing required property");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("verdict") && e.contains("required")),
            "{errors:?}"
        );
        // Nested instance paths make violations actionable.
        let c2 = ResultContract::compile(serde_json::json!({
            "type": "array",
            "items": { "type": "object", "required": ["n"] }
        }))
        .unwrap();
        let errors = c2
            .validate(&serde_json::json!([{ "n": 1 }, {}]))
            .expect_err("second item missing n");
        assert!(errors.iter().any(|e| e.contains("(at /1)")), "{errors:?}");
    }

    #[tokio::test]
    async fn two_concurrent_listeners_and_cleanup() {
        let (l1, p1) = bind_result_socket().await.expect("bind 1");
        let (l2, p2) = bind_result_socket().await.expect("bind 2");
        assert_ne!(p1, p2, "sockets must have distinct paths");
        assert!(p1.exists() && p2.exists(), "socket paths exist while open");

        drop(l1);
        drop(l2);
        // Dropping a listener leaves the file (only close() unlinks in
        // production); clean up here so the temp dir is not littered.
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
        let renderer = Arc::new(Renderer::new(crate::render::RenderOptions::quiet()));
        // Closing channels (the production cleanup path) unlinks paths.
        let (l3, p3) = bind_result_socket().await.expect("bind 3");
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let channel = spawn_result_channel(
            l3,
            contract(),
            Arc::new(|_| true),
            renderer.clone(),
            "t/s1".into(),
            cancel_tx,
        );
        assert!(p3.exists());
        channel.close().await;
        assert!(!p3.exists(), "close() removes the socket path");
        // After close, connections are refused.
        assert!(UnixStream::connect(&p3).await.is_err());
    }

    #[tokio::test]
    async fn stale_socket_is_unlinked_and_rebound() {
        let dir = std::env::temp_dir().join(format!("ponos-rc-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{SOCKET_PREFIX}deadbeef.sock"));

        // Stale: a plain file sits where the socket should be.
        std::fs::File::create(&path).unwrap();
        let listener = bind_at(&path)
            .await
            .expect("stale file is unlinked and rebound");
        drop(listener);
        let _ = std::fs::remove_file(&path);

        // Live: a real listener holds the name; bind must fail, not steal.
        let holder = UnixListener::bind(&path).unwrap();
        let err = bind_at(&path)
            .await
            .expect_err("live socket must not be stolen");
        assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
        drop(holder);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn submit_verdict_round_trip_over_socket() {
        let (listener, path) = bind_result_socket().await.expect("bind");
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let renderer = Arc::new(Renderer::new(crate::render::RenderOptions::quiet()));
        let in_flight = Arc::new(std::sync::Mutex::new(Some(())));
        let sink: SubmissionSink = {
            let in_flight = in_flight.clone();
            Arc::new(move |value| in_flight.lock().unwrap().is_some() && value.is_object())
        };
        let channel = spawn_result_channel(
            listener,
            contract(),
            sink,
            renderer,
            "t/s1".into(),
            cancel_tx,
        );

        let mut client = connect(&path).await.expect("connect");
        // Valid submission while a turn is in flight: accepted.
        let verdict = submit_over_socket(&mut client, &serde_json::json!({ "verdict": "approve" }))
            .await
            .expect("round trip");
        assert!(verdict.is_ok());
        // Invalid submission: verdict names the violation.
        let verdict = submit_over_socket(&mut client, &serde_json::json!({ "score": 1 }))
            .await
            .expect("round trip");
        let errors = verdict.expect_err("missing required property");
        assert!(errors.iter().any(|e| e.contains("verdict")), "{errors:?}");
        // The accepted submission counts.
        assert!(channel.any_accepted(), "accepted submission counts");
        // Late submission (no turn in flight): still an ok verdict (the
        // model already finished; a late submit is not a validation
        // failure), but it does not land anywhere observable.
        *in_flight.lock().unwrap() = None;
        let verdict = submit_over_socket(&mut client, &serde_json::json!({ "verdict": "late" }))
            .await
            .expect("round trip");
        assert!(verdict.is_ok());

        channel.close().await;
        assert!(!path.exists());
    }
}
