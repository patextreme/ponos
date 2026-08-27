//! The tokio `ProcessRunner` implementation funding `ponos.exec`:
//! spawn `/bin/sh -c` as its own process group (one kill reaches the
//! whole pipeline), capture stdout/stderr concurrently with the wait
//! (a child that fills a pipe can never deadlock the run), enforce the
//! timeout budget by SIGKILLing the group and reaping, and treat
//! *dropping* the returned future as cancellation: kill the group. This
//! is the composition-root twin of the ACP stdio transport — the two
//! places ponos's world-touching decisions live.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use ponos_core::ports::{ExecError, ExecOutcome, ProcessRunner};

/// The `ProcessRunner` used by the `ponos` CLI (stateless: every call
/// spawns its own child and group).
pub struct TokioProcessRunner;

/// Kills the whole process group on drop (or on [`GroupKillGuard::kill_now`])
/// unless disarmed. The child is its own group leader — spawned with
/// `process_group(0)` — so `-pid` reaches every process the command went
/// on to spawn, not just the shell.
struct GroupKillGuard {
    pid: Option<u32>,
}

impl GroupKillGuard {
    fn new(pid: u32) -> Self {
        Self { pid: Some(pid) }
    }

    /// Send SIGKILL to the group and disarm (a second kill is pointless;
    /// the disarm also marks the normal-exit path).
    fn kill_now(&mut self) {
        if let Some(pid) = self.pid.take() {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            #[cfg(not(unix))]
            {
                let _ = pid;
            }
        }
    }

    /// The command ended on its own: there is nothing to kill.
    fn disarm(&mut self) {
        self.pid = None;
    }
}

impl Drop for GroupKillGuard {
    fn drop(&mut self) {
        self.kill_now();
    }
}

/// Read a pipe to EOF as a (lossy, UTF-8) string.
async fn read_all<R: tokio::io::AsyncReadExt + Unpin>(mut r: R) -> std::io::Result<String> {
    let mut buf = Vec::new();
    r.read_to_end(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

impl ProcessRunner for TokioProcessRunner {
    fn run<'a>(
        &'a self,
        cmd: &'a str,
        timeout_ms: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<ExecOutcome, ExecError>> + Send + 'a>> {
        Box::pin(async move {
            use tokio::process::Command;

            // The child inherits ponos's environment and working
            // directory (no override options in v1); stdin is closed so
            // an interactive child reads EOF instead of hanging on — or
            // stealing — ponos's own stdin.
            let mut child = Command::new("/bin/sh")
                .arg("-c")
                .arg(cmd)
                .process_group(0)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| ExecError::Spawn(format!("`{cmd}`: {e}")))?;
            let pid = child.id().expect("child spawned");
            let mut guard = GroupKillGuard::new(pid);
            let mut stdout = child.stdout.take().expect("stdout piped");
            let mut stderr = child.stderr.take().expect("stderr piped");

            // Concurrent reads + wait: cancel-safe to drop at any point
            // (the guard performs the kill).
            let wait_and_read = async {
                let (out, err, status) =
                    tokio::join!(read_all(&mut stdout), read_all(&mut stderr), child.wait());
                (out.unwrap_or_default(), err.unwrap_or_default(), status)
            };

            let timed = match timeout_ms {
                Some(ms) => tokio::time::timeout(Duration::from_millis(ms), wait_and_read).await,
                None => Ok(wait_and_read.await),
            };

            match timed {
                Ok((stdout, stderr, status)) => {
                    guard.disarm();
                    Ok(ExecOutcome {
                        // A signal death is normalized to the shell's
                        // `128 + signal` convention, so `exit_code` is
                        // `Some` for every command that actually ran.
                        exit_code: status.ok().and_then(|s| exit_status_code(&s)),
                        stdout,
                        stderr,
                        timed_out: false,
                    })
                }
                Err(_elapsed) => {
                    // Budget spent: kill the whole group, then reap so no
                    // zombie remains before the outcome reports the
                    // timeout.
                    guard.kill_now();
                    let _ = child.wait().await;
                    Ok(ExecOutcome {
                        exit_code: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        timed_out: true,
                    })
                }
            }
        })
    }
}

/// `status.code()`, with a Unix signal death mapped to `128 + signal`.
fn exit_status_code(status: &std::process::ExitStatus) -> Option<i32> {
    status.code().or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt as _;
            status.signal().map(|sig| 128 + sig)
        }
        #[cfg(not(unix))]
        {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_pipeline_and_captures_output() {
        let out = TokioProcessRunner
            .run("printf 'a\\nb' | wc -l", None)
            .await
            .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), "1");
        assert!(!out.timed_out);
    }

    #[tokio::test]
    async fn nonzero_exit_and_stderr_are_data() {
        let out = TokioProcessRunner
            .run("echo boom >&2; exit 3", None)
            .await
            .unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.stdout, "");
        assert_eq!(out.stderr, "boom\n");
    }

    #[tokio::test]
    async fn timeout_kills_and_reports() {
        let started = std::time::Instant::now();
        let out = TokioProcessRunner.run("sleep 30", Some(100)).await.unwrap();
        assert!(out.timed_out);
        assert_eq!(out.exit_code, None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must not wait out the sleep"
        );
    }

    #[tokio::test]
    async fn signal_death_normalizes_to_shell_convention() {
        // The shell itself is killed by a signal: normalized to 128+sig
        // so `exit_code` is `Some` for every command that ran.
        let out = TokioProcessRunner.run("kill -9 $$", None).await.unwrap();
        assert_eq!(out.exit_code, Some(137));
    }

    #[tokio::test]
    async fn stdin_is_eof_not_the_terminal() {
        // `cat` reads stdin until EOF; with stdin nulled it exits at once.
        let started = std::time::Instant::now();
        let out = TokioProcessRunner.run("cat", Some(5_000)).await.unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout, "");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cat must see EOF immediately"
        );
    }

    #[tokio::test]
    async fn env_and_cwd_are_inherited() {
        // SAFETY: single-threaded test; the var is ours.
        unsafe { std::env::set_var("PONOS_EXEC_TEST_TOKEN", "tok-42") };
        let out = TokioProcessRunner
            .run("printf %s \"$PONOS_EXEC_TEST_TOKEN\"; pwd", None)
            .await
            .unwrap();
        let expected = format!("tok-42{}\n", std::env::current_dir().unwrap().display());
        assert_eq!(out.stdout, expected);
    }

    #[tokio::test]
    async fn child_filling_a_pipe_does_not_deadlock() {
        // 1 MB through the pipe: concurrent reads + wait must drain it.
        let out = TokioProcessRunner
            .run(
                "yes 0123456789 | head -c 1048576 >/dev/null; echo done",
                None,
            )
            .await
            .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), "done");
    }
}
