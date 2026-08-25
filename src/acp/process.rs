//! Agent subprocess management: spawn, the stderr pump, and teardown
//! kill/reap.

use std::sync::Arc;

use agent_client_protocol::AcpAgent;
use agent_client_protocol::schema::v1::{EnvVariable, McpServer, McpServerStdio};

use crate::core::config::AgentSpec;
use crate::core::events::SessionEvent;
use crate::core::ports::EventSink;

use super::SessionError;

/// One spawned agent subprocess plus its stderr pump.
pub(super) struct AgentProcess {
    pub stdin: async_process::ChildStdin,
    pub stdout: async_process::ChildStdout,
    /// OS process id of the agent subprocess.
    pub pid: u32,
    pub child: async_process::Child,
    /// The `-vv` stderr passthrough task; awaited at teardown so
    /// passthrough completes before the session is reported closed.
    pub stderr_task: tokio::task::JoinHandle<()>,
}

/// Start one agent subprocess as described by `spec` (attributed to
/// `label`) and pump its stderr to the sink line by line.
pub(super) fn spawn(
    spec: &AgentSpec,
    label: &str,
    sink: Arc<dyn EventSink>,
) -> Result<AgentProcess, SessionError> {
    let env = spec
        .env
        .iter()
        .map(|(k, v)| EnvVariable::new(k.clone(), v.clone()))
        .collect::<Vec<_>>();

    let server = McpServer::Stdio(
        McpServerStdio::new(label.to_string(), spec.command.clone())
            .args(spec.args.clone())
            .env(env),
    );
    let agent = AcpAgent::new(server);

    let (stdin, stdout, stderr, child) = agent
        .spawn_process()
        .map_err(|e| SessionError::Spawn(format!("`{}`: {e}", spec.command)))?;
    let pid = child.id();

    let stderr_label = label.to_string();
    let stderr_sink = sink;
    let stderr_task = tokio::spawn(async move {
        use futures::AsyncBufReadExt;
        use futures::StreamExt;
        let mut lines = futures::io::BufReader::new(stderr).lines();
        while let Some(Ok(line)) = lines.next().await {
            stderr_sink.emit(&stderr_label, SessionEvent::StderrLine { line });
        }
    });

    Ok(AgentProcess {
        stdin,
        stdout,
        pid,
        child,
        stderr_task,
    })
}

/// Kill the child's whole process group (agents are commonly launched via
/// `npx`-style wrappers) and reap it so no zombie remains.
pub(super) async fn kill_and_reap(mut child: async_process::Child) {
    #[cfg(unix)]
    unsafe {
        let pid = child.id() as i32;
        // The child is its own process-group leader (spawn sets this).
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.status().await; // reap
}
