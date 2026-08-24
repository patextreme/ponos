## MODIFIED Requirements

### Requirement: Agent and session API
The `ponos` namespace SHALL provide `ponos.agent(name_or_spec)` returning an agent factory, and `agent:session(options)` returning a session object. Each `session()` call creates an independent session with its own agent subprocess. Session options SHALL accept `cwd` (resolved relative to the invocation directory), `id` (label used in output attribution, defaulting to `s1`, `s2`, … per agent), `mcp_servers`, and `result` (a JSON Schema expressed as a Luau table; the option's semantics are specified by the typed-results capability). Two `ponos.agent` calls for the same name SHALL return independent factory objects.

#### Scenario: Session creation
- **WHEN** a script calls `ponos.agent("claude"):session({ id = "reviewer" })`
- **THEN** a session labeled `claude/reviewer` exists and is ready to prompt

#### Scenario: Default session labels
- **WHEN** two sessions are created without `id` from the same agent factory
- **THEN** they are labeled `s1` and `s2` respectively in output attribution

#### Scenario: Independent factories
- **WHEN** `ponos.agent("claude")` is called twice with the same name and each factory creates a session
- **THEN** the factories keep independent session counters: both first sessions are labeled `claude/s1`

#### Scenario: Unknown agent name
- **WHEN** a script calls `ponos.agent("nope")` and `nope` exists in no registry
- **THEN** a Lua error is raised naming the unresolved agent

### Requirement: Prompt returns a result table
`session:prompt(text, options?)` SHALL send one prompt turn and return a table with `text` (final agent message string), `stop_reason` (`"end_turn"`, `"max_tokens"`, `"max_turn_requests"`, `"refusal"`, or `"cancelled"`), `usage` (`input`, `cache_read`, `cache_write`, `output` token counts, zero when unreported), and `result` (the turn's last accepted typed submission converted to a Luau value; `nil` when the session declared no contract or the turn had no accepted submission — the field's semantics are specified by the typed-results capability). The result table SHALL be directly string-coercible to `text` via `__tostring`. Options SHALL accept `timeout_ms`.

#### Scenario: Successful turn
- **WHEN** `local r = s:prompt("hi")` completes normally
- **THEN** `r.text` is the agent's final message, `tostring(r)` equals `r.text`, and `r.stop_reason == "end_turn"`

#### Scenario: Timeout is an error
- **WHEN** `s:prompt("...", { timeout_ms = 50 })` exceeds its timeout
- **THEN** the turn is cancelled via `session/cancel` and the call raises a catchable Lua timeout error
