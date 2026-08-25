//! Wire protocol plumbing: typed request/response resolution and the
//! `initialize` / `session/new` handshake with capability negotiation.

use std::path::PathBuf;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    BooleanConfigOptionCapabilities, ClientCapabilities, ClientSessionCapabilities,
    InitializeRequest, McpServer, NewSessionRequest, SessionConfigOption,
    SessionConfigOptionsCapabilities,
};
use agent_client_protocol::{ConnectionTo, JsonRpcRequest};
use tokio::sync::oneshot;

/// The session established by a successful handshake.
pub(super) struct Handshake {
    pub session_id: agent_client_protocol::schema::v1::SessionId,
    /// Config options advertised at `session/new` (the session's initial
    /// option state).
    pub config_options: Option<Vec<SessionConfigOption>>,
}

/// Perform the `initialize` handshake and `session/new`.
///
/// ponos declares exactly one client capability — the non-interactive
/// `session.configOptions` (with its `boolean` sub-capability, so
/// conforming agents may offer boolean options and accept boolean set
/// values). It commits ponos to nothing interactive; agent-to-client
/// requests are still answered by the driver's dispatch handler.
pub(super) async fn handshake(
    conn: &ConnectionTo<agent_client_protocol::Agent>,
    cwd: PathBuf,
    mcp_servers: Vec<McpServer>,
) -> Result<Handshake, String> {
    let mut init = InitializeRequest::new(ProtocolVersion::V1);
    init.client_capabilities =
        ClientCapabilities::new().session(Some(ClientSessionCapabilities::new().config_options(
            SessionConfigOptionsCapabilities::new().boolean(BooleanConfigOptionCapabilities::new()),
        )));
    request(conn, init).await?;

    let mut new_session_req = NewSessionRequest::new(cwd);
    new_session_req.mcp_servers = mcp_servers;
    let resp = request(conn, new_session_req).await?;
    Ok(Handshake {
        session_id: resp.session_id,
        config_options: resp.config_options,
    })
}

/// Send a typed request and resolve its response through a oneshot.
pub(super) async fn request<Req: JsonRpcRequest>(
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
