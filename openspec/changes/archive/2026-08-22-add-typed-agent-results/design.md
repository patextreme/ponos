## Context

Today `TurnOutcome` (src/acp/mod.rs) carries only assembled text, stop reason, and usage: ACP's `PromptResponse` has no structured-output slot, and ponos cannot inject custom tools into agents it merely drives. The two channels ponos does control are `session/new { mcpServers }` (client-suggested MCP servers; stdio is the only transport all agents must support) and the JSON-RPC connection itself. The agentflow POC (pi extension) validated the mechanism being ported: a schema-gated submit tool executed orchestrator-side, with in-turn validation errors driving model retry. Its primitives map onto ACP as: custom tool → injected MCP server; SDK tool-arg validation → validation in ponos-main over the transport; submission slot → per-turn slot in the turn accumulator.

Constraints that shape the design: the suite must stay fully offline (mock-agent only); the runtime is single-threaded Lua-side on a `LocalSet`; process teardown already kills whole process groups under `cfg(unix)`; ponos has no HTTP stack and adding listeners on macOS triggers firewall prompts.

## Goals / Non-Goals

**Goals:**
- Round-trip fidelity with the POC's semantics: schema-gated tool, last-wins slot, in-turn retry on violation, `nil` when absent.
- Work with any spec-conformant ACP agent without agent-specific configuration.
- Zero-config opt-in: setting `result` is the entire user surface.

**Non-Goals:**
- Per-turn schemas (agents may cache `tools/list`; revisit with schema-push only if needed).
- Windows support (UDS; named pipes would be a separate change).
- A schema-builder DSL — raw JSON Schema Luau tables are the interface; sugar can be a user module later.
- Prompt-contract fallback (parse JSON out of final text) — a possible degraded mode, not built here.
- `McpServer::Http` exposure of the submit tool (future fast path for agents advertising `mcp_capabilities.http`).
- Depending on `unstable_mcp_over_acp`; watch it, don't use it.

## Decisions

### D1: Transport — per-session UDS, newline-delimited JSON

Each result session binds a Unix domain socket at creation: `$XDG_RUNTIME_DIR` (Linux) or `$TMPDIR` (macOS), filename `ponos-r-<32 hex>.sock`. Protocol is one JSON object per line: `{"op":"submit","value":…}` → `{"ok":true}` or `{"ok":false,"errors":[…]}`. The submit call blocks on the verdict — that round-trip *is* the in-turn retry mechanism.

Chosen over localhost HTTP: no ports (no bind races), no macOS firewall prompt, filesystem permissions gate connections to the same user, zero new transport dependencies (`tokio` full already ships `UnixListener`/`UnixStream`). HTTP's only advantage, curl-debuggability, is moot when both endpoints are ponos. macOS notes: no abstract sockets (filesystem path required — hence the temp dir), `sun_path` is 104 bytes (short generated filename; rules out descriptive names). Stale-socket case (path exists, no listener) is detected at bind and unlinked before rebinding; socket is unlinked at session close.

The socket path doubles as the capability token: it is passed only to that session's agent via the injected server's env (`PONOS_BRIDGE_ADDR`), unguessable without it. `SO_PEERCRED`-style peer-UID checks are optional hardening, not v1 scope.

### D2: Bridge — hidden subcommand re-executing the main binary

The injected `McpServerStdio` entry is `{ name: "ponos", command: current_exe(), args: ["__bridge"], env: { PONOS_BRIDGE_ADDR, PONOS_RESULT_SCHEMA } }`. The agent spawns it per `session/new`; it speaks MCP server over stdio and relays `tools/call` to the UDS. It holds no state beyond its env, never talks to the model, and exits on stdin EOF — which is exactly the teardown signal when the agent dies or closes the session.

Embedding the schema in env (rather than a UDS fetch at startup) keeps the bridge stateless and kills a protocol message plus an ordering race; schemas are small against env limits and are not secret (they came from the script). The same assumption makes `current_exe()` safe: ponos's agents are already local same-filesystem subprocesses; Nix store paths are immutable so mid-run rebuilds don't invalidate the path. If the assumption fails (sandboxed agent), the failure is the designed degradation path, not a hang.

Tool name `result_submit`, server name `ponos` → derived `mcp__ponos__result_submit` in agent transcripts is the greppable identity.

### D3: MCP implementation — rmcp on both sides

The bridge is an rmcp MCP server; mock-agent is an rmcp MCP client. Correctness and interoperability are the priority (user decision): version negotiation and framing for free, and the offline suite then exercises the same SDK pair real agents using official SDKs will. Alternative rejected: hand-rolled JSON-RPC subset (~200 lines) — consistent with the hand-rolled ACP client, but it would test code no real agent ever runs.

### D4: Validation — `jsonschema` crate, in ponos-main

Submissions must reach ponos-main anyway (the slot lives there), so the verdict is computed there: one validator, one truth, best-in-class error messages. Error text (`missing property 'score' at /items/1`-style) flows verbatim into the MCP tool error — message quality *is* the retry UX, which rules out a hand-rolled subset validator. The schema itself is compiled eagerly at `agent:session()` so authoring errors fail at the author's line, before any subprocess spawns; remote `$ref`s are rejected at the same point (offline guarantee).

Luau→JSON: the schema table converts via mlua serialization (`LuaSerdeExt`, already used for `mcp_servers`). JSON→Luau on the way back: objects→tables, arrays→tables, numbers→numbers, `null`/absent→`nil`. Known Luau quirks to document: `{}` serializes as an object (not an array), so schema-side empty arrays should use explicit array-typed constructs when needed; `integer` and `number` both arrive as Luau numbers.

### D5: Permissions — allow-all, every session, `AllowAlways`

ACP `ToolCall` carries no tool name (only id, agent-written `title`, kind, raw input/output), so per-tool identification is impossible at the protocol level; heuristics on titles are unreliable both ways. Since ponos is headless — there is never a human to ask, and a denied tool silently degrades output — every `session/request_permission` gets an offered allow option: the first `AllowAlways` when present, otherwise the first other allow-kind option (e.g. `AllowOnce` — notably the offer shape mock-agent itself has historically used) (user decision, generalizing beyond result sessions). This replaces the deny-all `-32601` dispatch for permission requests only; elicitation/fs/terminal stay unsupported-error. If an agent offers no allow-kind option at all (only reject options, or none), respond with the unsupported-method error (cannot select an allow that doesn't exist). Documented consequence: `AllowAlways` may persist allow rules in the agent's own settings beyond the run — usually desirable for headless/CI, stated plainly in the README.

### D6: Slot semantics and turn wiring

The submission slot lives in the turn accumulator (`TurnFold`), keyed to the in-flight turn: cleared when a prompt starts; set only on `ok` verdicts (last-wins); taken into `TurnOutcome.result` when the prompt response lands; discarded on cancel/timeout paths (the existing cancel/timeout branches already drop the fold's text the same way). Submissions arriving with no in-flight turn (late model finish, lingering bridge) are dropped with one renderer lifecycle line. No `last_result()` accessor — result belongs to a turn. Prompt augmentation: one fixed sentence appended in `run_turn` when the session has a contract; the schema never enters prompt text (the tool carries it).

### D7: Testing — mock-agent as MCP client

mock-agent honors `session/new` `mcpServers` by spawning configured stdio servers with rmcp, handshaking, and exposing scripted behavior through flags: `MOCK_SUBMIT=<json>` (submit once), `MOCK_SUBMIT_BAD=<n>` (submit n violating values, assert tool errors, then submit valid — proving in-turn retry), `MOCK_NO_MCP` (ignore servers entirely — spec-legal degradation). Concurrency coverage via a plain test running two result sessions side by side. A worked example (schema module + fan-out consumer) joins `examples/` with its `tests/examples.rs` entry, per repo convention.

## Risks / Trade-offs

- [Agents may ignore suggested `mcpServers` or mishandle session-scoped servers] → Designed degradation: `result = nil` + one lifecycle log; never an error or hang. `MOCK_NO_MCP` pins this path in the suite.
- [Permission allow-all auto-approves dangerous tools on gating agents] → Headless rationale accepted by decision; blast radius equals the agent's own config (an agent gating submit would gate everything anyway). README states the contract change and the persistence side effect. Rollback of D5 alone is trivial (revert dispatch to method-not-found).
- [rmcp and jsonschema are new dependency trees] → Both widely used; pinned in Cargo.toml; no feature-flag gymnastics expected. If rmcp proves heavyweight, the bridge's 4-method surface is small enough to hand-roll later without touching the UDS protocol.
- [Env-borne schema size limits] → Schemas are tens of KB at most; the eager compile catches garbage early. A schema large enough to blow env limits fails at session creation with a clear error.
- [macOS `sun_path` length / temp-dir quirks] → Short generated filename; `$TMPDIR` fallback; stale-socket unlink-and-rebind.
- [Bridge subprocess leaks if the agent never reaps it] → It exits on stdin EOF by construction; ponos's process-group kill already covers the tree at teardown.
- [Luau `{}`-as-object / null-as-nil footguns] → Documented in README mapping table; example script demonstrates the canonical patterns.

## Migration Plan

No data or config migration. Ship order within the change: UDS + slot + outcome plumbing first (text-visible only), then bridge + injection, then rmcp mock-client flags, then type definitions + example + README. Each step keeps the suite green; the permission posture change lands with its own spec delta and README rewrite. Rollback is per-decision: each of D1–D7 is separable, D5 being the only one altering existing-session behavior.

## Open Questions

- Exact wording of the fixed prompt-augmentation sentence (spec pins presence, not wording) — decide during implementation against mock transcripts.
- Whether to add `SO_PEERCRED`/`LOCAL_PEERCRED` peer checks — optional hardening, additive later.
