//! The hidden `ponos __bridge` subcommand: an MCP server over stdio that
//! relays `result_submit` calls to ponos-main over the session's result
//! socket.
//!
//! The agent (MCP client) spawns one bridge per result session, as
//! suggested in `session/new { mcpServers }`. The bridge is stateless
//! beyond its environment: `PONOS_BRIDGE_ADDR` is the session's
//! Unix-domain result socket and `PONOS_RESULT_SCHEMA` the declared JSON
//! Schema (traveling as the tool's input schema so the model sees it
//! without it ever entering prompt text). On `tools/call` it forwards the
//! `value` argument over the socket and blocks for the verdict, which
//! ponos-main computes by validating the value against the schema. A
//! violation comes back as a tool error naming the violations — the model
//! sees it and can correct the value inside the same turn.
//!
//! The bridge exits when stdin closes (the agent died or closed the
//! session): exactly the teardown signal for the whole channel.

use std::sync::Arc;

use rmcp::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::serve_server;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::io::stdio;

use crate::result_contract;

/// Name of the injected server (agents derive `mcp__ponos__result_submit`).
pub const SERVER_NAME: &str = "ponos";
/// Name of the single tool the bridge exposes.
pub const TOOL_NAME: &str = "result_submit";

/// Wrap a declared schema as the tool's input schema: one required `value`
/// property carrying the declared schema. Wrapping keeps any root schema
/// shape (object, string, array, …) expressible — MCP `inputSchema` itself
/// must be an object.
pub fn wrap_input_schema(schema: &serde_json::Value) -> JsonObject {
    let wrapped = serde_json::json!({
        "type": "object",
        "properties": { "value": schema },
        "required": ["value"],
    });
    match wrapped {
        serde_json::Value::Object(map) => map,
        _ => unreachable!("object literal"),
    }
}

/// The tool listing (shared with tests).
pub fn tool_for(schema: &serde_json::Value) -> Tool {
    let description = "Call this when your work on the task is complete, with the final \
         result as the `value` argument. The `value` argument must satisfy the \
         session's declared JSON Schema; violations are reported back so you \
         can correct the value and submit again.";
    let mut tool = Tool::default();
    tool.name = TOOL_NAME.into();
    tool.title = Some("Submit typed result".into());
    tool.description = Some(description.into());
    tool.input_schema = Arc::new(wrap_input_schema(schema));
    tool
}

/// The MCP server: schema from env, verdicts relayed over the socket.
#[derive(Clone)]
struct BridgeServer {
    schema: serde_json::Value,
    socket: std::path::PathBuf,
    /// Lazily established persistent connection to ponos-main.
    conn: Arc<tokio::sync::Mutex<Option<tokio::net::UnixStream>>>,
}

impl BridgeServer {
    async fn relay(&self, value: &serde_json::Value) -> Result<Result<(), Vec<String>>, String> {
        let mut guard = self.conn.lock().await;
        if guard.is_none() {
            *guard = Some(
                result_contract::connect(&self.socket)
                    .await
                    .map_err(|e| format!("cannot reach result channel: {e}"))?,
            );
        }
        let stream = guard.as_mut().expect("just connected");
        match result_contract::submit_over_socket(stream, value).await {
            Ok(verdict) => Ok(verdict),
            Err(e) => {
                // Stale connection (session closed underneath): drop it so
                // the next call reconnects.
                *guard = None;
                Err(format!("result channel error: {e}"))
            }
        }
    }
}

impl ServerHandler for BridgeServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        let mut implementation = Implementation::default();
        implementation.name = SERVER_NAME.to_string();
        implementation.version = crate::VERSION.to_string();
        info.server_info = implementation;
        info
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        let listing = ListToolsResult::with_all_items(vec![tool_for(&self.schema)]);
        std::future::ready(Ok(listing))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        (name == TOOL_NAME).then(|| tool_for(&self.schema))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, rmcp::ErrorData>> + Send + '_ {
        let relay = self.clone();
        async move {
            if request.name != TOOL_NAME {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "unknown tool: {} (this server exposes only {TOOL_NAME})",
                    request.name
                ))])
                .into());
            }
            let Some(arguments) = request.arguments else {
                return Ok(CallToolResult::error(vec![ContentBlock::text(
                    "missing required argument 'value'",
                )])
                .into());
            };
            let Some(value) = arguments.get("value") else {
                return Ok(CallToolResult::error(vec![ContentBlock::text(
                    "missing required argument 'value'",
                )])
                .into());
            };
            match relay.relay(value).await {
                Ok(Ok(())) => {
                    Ok(CallToolResult::success(vec![ContentBlock::text("result accepted")]).into())
                }
                Ok(Err(errors)) => Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "result rejected by schema: {}",
                    errors.join("; ")
                ))])
                .into()),
                Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(e)]).into()),
            }
        }
    }
}

/// Entry point for `ponos __bridge`. Runs until stdin closes. Returns the
/// process exit code.
pub fn run() -> std::process::ExitCode {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    match rt.block_on(run_async()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ponos __bridge: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run_async() -> Result<(), String> {
    let socket = std::env::var_os("PONOS_BRIDGE_ADDR")
        .map(std::path::PathBuf::from)
        .ok_or("PONOS_BRIDGE_ADDR is not set (this subcommand is spawned by agents)")?;
    let schema_raw = std::env::var("PONOS_RESULT_SCHEMA")
        .map_err(|_| "PONOS_RESULT_SCHEMA is not set".to_string())?;
    let schema: serde_json::Value = serde_json::from_str(&schema_raw)
        .map_err(|e| format!("invalid PONOS_RESULT_SCHEMA: {e}"))?;

    let server = BridgeServer {
        schema,
        socket,
        conn: Arc::new(tokio::sync::Mutex::new(None)),
    };
    let service = serve_server(server, stdio())
        .await
        .map_err(|e| format!("failed to start MCP server: {e}"))?;
    // Block until the agent closes stdio.
    service
        .waiting()
        .await
        .map_err(|e| format!("server task failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_description_carries_submit_guidance() {
        // Spec "Tool description carries the submit guidance": the tool's
        // description must tell the agent when to call it and how the
        // result is passed, so the guidance survives without any
        // prompt-side injection.
        let tool = tool_for(&serde_json::json!({"type": "object"}));
        let description = tool.description.as_deref().expect("description set");
        assert!(
            description.contains("when your work"),
            "missing submit timing: {description}"
        );
        assert!(
            description.contains("`value` argument"),
            "missing value-argument naming: {description}"
        );
    }
}
