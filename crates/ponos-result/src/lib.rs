//! The result channel I/O: the per-session Unix-domain-socket channel and
//! the newline-JSON submit/verdict protocol shared by ponos-main and the
//! hidden `ponos __bridge` subcommand.
//!
//! ponos-main binds a per-session socket and offers the agent an MCP
//! server (the bridge) whose single `result_submit` tool relays
//! submissions over that socket. ponos-main validates each submission
//! against the compiled contract ([`ponos_core::contract`]) and answers
//! with the verdict — that blocking round-trip is the in-turn retry
//! mechanism: a violation is a tool error the model sees and can fix
//! inside the same turn.
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

use ponos_core::contract::{ResultContract, SubmissionSink};
use ponos_core::events::SessionEvent;
use ponos_core::ports::EventSink;

/// Filename prefix for per-session result sockets (`ponos-r-<32hex>.sock`).
const SOCKET_PREFIX: &str = "ponos-r-";

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
    /// Stop the channel: cancel the accept loop, wait (bounded) for it
    /// to finish, and unlink the socket path.
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
    event_sink: Arc<dyn EventSink>,
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
                        let event_sink = event_sink.clone();
                        let label = label.clone();
                        let any = any_task.clone();
                        tokio::spawn(async move {
                            serve_connection(stream, contract, sink, event_sink, label, any)
                                .await;
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
    event_sink: Arc<dyn EventSink>,
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
                let late = !sink(value);
                if !late {
                    any_accepted.store(true, Ordering::Relaxed);
                }
                event_sink.emit(
                    &label,
                    SessionEvent::ResultVerdict {
                        accepted: true,
                        late,
                    },
                );
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
            Err(errors) => {
                // A violation is a tool error the model sees and can fix
                // inside the same turn; nothing to render.
                event_sink.emit(
                    &label,
                    SessionEvent::ResultVerdict {
                        accepted: false,
                        late: false,
                    },
                );
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
    use ponos_core::contract::ResultContract;
    use ponos_core::events::SessionEvent;

    /// A recording sink: captures events so tests assert on them instead
    /// of leaning on the terminal renderer.
    #[derive(Default)]
    struct RecordingSink(std::sync::Mutex<Vec<(String, SessionEvent)>>);

    impl EventSink for RecordingSink {
        fn emit(&self, label: &str, event: SessionEvent) {
            self.0.lock().unwrap().push((label.to_string(), event));
        }
        fn script_log(&self, _message: &str) {}
    }

    fn contract() -> ResultContract {
        ResultContract::compile(serde_json::json!({
            "type": "object",
            "properties": { "verdict": { "type": "string" }, "score": { "type": "integer" } },
            "required": ["verdict"]
        }))
        .expect("schema compiles")
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
        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        // Closing channels (the production cleanup path) unlinks paths.
        let (l3, p3) = bind_result_socket().await.expect("bind 3");
        let (cancel_tx, _cancel_rx) = watch::channel(false);
        let channel = spawn_result_channel(
            l3,
            contract(),
            Arc::new(|_| true),
            sink.clone(),
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
        let event_sink = Arc::new(RecordingSink::default());
        let events = event_sink.clone();
        let in_flight = Arc::new(std::sync::Mutex::new(Some(())));
        let sink: SubmissionSink = {
            let in_flight = in_flight.clone();
            Arc::new(move |value| in_flight.lock().unwrap().is_some() && value.is_object())
        };
        let channel = spawn_result_channel(
            listener,
            contract(),
            sink,
            event_sink as Arc<dyn EventSink>,
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

        // The verdicts were emitted through the sink: accepted, rejected,
        // and late (dropped) in order.
        let recorded = events.0.lock().unwrap().clone();
        assert_eq!(recorded.len(), 3, "{recorded:?}");
        assert!(matches!(
            &recorded[0],
            (label, SessionEvent::ResultVerdict { accepted: true, late: false }) if label == "t/s1"
        ));
        assert!(matches!(
            &recorded[1],
            (
                _,
                SessionEvent::ResultVerdict {
                    accepted: false,
                    late: false
                }
            )
        ));
        assert!(matches!(
            &recorded[2],
            (
                _,
                SessionEvent::ResultVerdict {
                    accepted: true,
                    late: true
                }
            )
        ));

        channel.close().await;
        assert!(!path.exists());
    }
}
