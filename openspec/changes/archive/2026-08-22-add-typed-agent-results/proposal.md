## Why

ponos scripts can only get prose back from agents: `session:prompt()` returns assembled message text, and extracting structured data means parsing model output by hand in every script. The agentflow POC (pi extension) proves the better pattern — a schema-gated `submit` tool whose execution lands in the orchestrator, with in-turn validation and retry — but ponos cannot inject custom tools into agents it doesn't own. ACP's `session/new { mcpServers }` field is the spec-sanctioned injection channel, and stdio is the only transport every agent must support, which makes an MCP round-trip the honest port of the POC's mechanism.

## What Changes

- **Typed result contracts.** `agent:session({ result = <schema> })` accepts a JSON Schema as a plain Luau table. Scripts read `out.result` from `session:prompt()` outcomes: the last submitted value as a Luau value, `nil` when the agent never submitted (or the turn was cancelled/timed out). No built-in retry; authors write retry loops in Luau.
- **Injected MCP bridge.** When `result` is set, ponos appends itself to `session/new`'s `mcpServers`: `current_exe()` re-executed as a hidden `ponos __bridge` subcommand, an MCP server on the agent's stdio exposing one tool, `result_submit`, carrying the schema as its `inputSchema` (wrapped as `{ value: <schema> }` so any root schema works). The agent (MCP client) spawns and drives it; submissions relay to ponos-main over a per-session Unix domain socket. The server is named `ponos`, so agents that derive names emit `mcp__ponos__result_submit`.
- **Blocking validation round-trip.** Each `result_submit` call reaches ponos-main over the UDS, is validated there with the `jsonschema` crate, and the verdict returns as the MCP tool result: a schema violation is a tool error the model can see and fix *inside the same turn* — the retry loop that makes typed results reliable.
- **Prompt augmentation.** When `result` is set, ponos appends one fixed sentence to each prompt telling the agent to call the tool; the schema itself travels in the tool, not the prompt.
- **Schema fail-fast.** The schema is compiled eagerly at `agent:session()` — an invalid schema (or remote `$ref`, which is rejected to keep runs offline) raises a Lua error at the author's line.
- **Slot semantics.** Last submission wins; the slot clears when a prompt starts; cancelled/timed-out turns discard any submission; submissions arriving after a turn settles are dropped with one lifecycle log line. No `session:last_result()` accessor.
- **BREAKING (behavior): headless permission posture.** ponos answers every `session/request_permission` — on every session, not only result sessions — with an offered allow option, preferring the first `AllowAlways` and falling back to the first other allow-kind option (e.g. `AllowOnce`), replacing today's deny-all `-32601`. Rationale: ponos runs headless; nobody is there to be asked, and a denied tool silently degrades output. When no allow-kind option is offered at all, ponos keeps responding with the unsupported-method error. Selecting `AllowAlways` may persist in the agent's own settings beyond the run; this is documented. Elicitation, fs, and terminal requests stay unsupported-error.
- **Degradation is designed, not exceptional.** If the agent ignores `mcpServers` (spec-legal), is sandboxed away from the binary, or the bridge never connects: `result = nil` plus one lifecycle log line. Never an error, never a hang.
- **Testing.** mock-agent grows an MCP client (rmcp) and new flags: `MOCK_SUBMIT` (submit a JSON value), `MOCK_SUBMIT_BAD` (submit N invalid values, then a valid one — proving in-turn retry), `MOCK_NO_MCP` (ignore injected servers). A concurrency test drives two result sessions side by side. A worked example script joins `examples/` with its `tests/examples.rs` entry.
- **Scope limits.** Per-session schema (per-turn deferred — agents cache `tools/list`); Unix-only (UDS), matching the existing `cfg(unix)` teardown; no `config.toml` surface — injection is automatic when `result` is set; raw JSON Schema tables only, no bundled builder DSL.

## Capabilities

### New Capabilities
- `typed-results`: schema-declared typed results for agent sessions — the `result` session option, the injected MCP bridge and UDS transport, submit/validate/retry semantics, slot and turn lifecycle rules, degradation behavior.

### Modified Capabilities
- `agent-sessions`: the "No client capabilities are exposed" requirement changes — `session/request_permission` is now answered with an offered allow option on every session (preferring `AllowAlways`, falling back to the first other allow-kind option, unsupported-method only when no allow option exists) instead of an unsupported-method error; other request types are unchanged.
- `scripting`: the session-options enumeration gains `result` (a JSON Schema expressed as a Luau table) and the prompt result-table enumeration gains a `result` field (the turn's accepted submission as a Luau value, `nil` when there was none); the option/field semantics themselves are specified by the new `typed-results` capability.
- `type-definitions`: definitions cover the typed-results surface — the `result` session option (optional string-keyed JSON Schema table) and the prompt-result `result` field (optional, `nil` when no accepted submission); the runtime probe and strict-analysis check grow to match.

## Impact

- `src/acp/` — `session/new` gains the injected `mcpServers` entry; permission dispatch gains the allow-all response; `TurnOutcome` carries `result`; UDS listener lifecycle per result session.
- `src/script/` — `agent:session({result=…})` option, eager schema compilation, `prompt()` outcome table gains `result`, prompt augmentation, Luau↔JSON conversion of submitted values.
- New bridge module (subcommand of the `ponos` binary) — rmcp MCP server over stdio, UDS client.
- `src/bin/mock-agent/` — rmcp MCP client honoring `session/new` mcpServers; `MOCK_SUBMIT`, `MOCK_SUBMIT_BAD`, `MOCK_NO_MCP` flags.
- New dependencies: `rmcp` (MCP server+client), `jsonschema` (validation in ponos-main).
- `examples/` + `tests/examples.rs` — worked typed-results example; `tests/` — bridge, retry, degradation, concurrency coverage.
- `types/ponos.d.luau` + `tests/fixtures/types_probe.luau` — `result` option and outcome `result` field typed; probe covers a result session end to end; the `ponos-analyze` strict check extends over the new example.
- `README.md` — `result` option documented; permission-posture contract rewritten.
- Non-goals recorded in design: per-turn schemas, Windows named pipes, prompt-contract fallback, `McpServer::Http` fast path (future), `unstable_mcp_over_acp` (watch, don't depend).
