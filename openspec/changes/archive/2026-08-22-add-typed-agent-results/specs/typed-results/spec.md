## Purpose

Lets scripts declare a typed result contract per agent session and receive a validated, structured value back from each prompt turn, instead of parsing agent prose.

## ADDED Requirements

### Requirement: Result contract declaration
`agent:session(options)` SHALL accept a `result` option whose value is a JSON Schema expressed as a Luau table. The schema SHALL be compiled at session-creation time; a schema that fails to compile SHALL raise a Lua error at the `session()` call site naming the compile failure. Schemas containing a remote `$ref` (a reference that is not a local JSON pointer within the same document) SHALL be rejected at the same point, so runs stay offline. Sessions created without `result` SHALL behave exactly as before, with no injected tool and no prompt augmentation.

#### Scenario: Valid schema accepted
- **WHEN** `session({ result = { type = "object", properties = { verdict = { type = "string" } }, required = { "verdict" } } })` is called
- **THEN** the session is created and the schema governs all prompts on it

#### Scenario: Invalid schema fails at the author's line
- **WHEN** `session({ result = { type = "objekt" } })` is called
- **THEN** a Lua error is raised from the `session()` call naming the schema problem

#### Scenario: Remote reference rejected
- **WHEN** the schema contains `$ref: "https://example.com/schema.json"`
- **THEN** a Lua error is raised from the `session()` call, before any agent subprocess is spawned

#### Scenario: Any root schema shape
- **WHEN** `result` is a non-object schema such as `{ type = "string", enum = { "ship", "block" } }`
- **THEN** the session accepts it and submissions are strings, not wrapped objects

### Requirement: Submit tool injection
When `result` is set, session creation SHALL offer the agent one additional MCP server over stdio, named `ponos`, exposing exactly one tool named `result_submit`. The tool's input schema SHALL be the declared schema wrapped under a single `value` property, so agents derive the call as `mcp__ponos__result_submit` and the declared schema reaches the model through the tool itself. Each prompt on a result session SHALL have one fixed instruction appended telling the agent to submit its final result by calling the tool; the schema SHALL NOT be inlined into prompt text. The injected server SHALL NOT change sessions that declare no `result`.

#### Scenario: Tool appears with wrapped schema
- **WHEN** a result session's agent lists tools from the injected server
- **THEN** exactly one tool `result_submit` is listed, whose input schema is `{ value: <declared schema> }`

#### Scenario: Prompt carries the instruction
- **WHEN** a prompt is sent on a result session
- **THEN** the text the agent receives ends with a fixed sentence instructing use of the submit tool

### Requirement: Typed prompt outcomes
The table returned by `session:prompt()` on a result session SHALL include a `result` field: the last accepted submission for that turn, converted from JSON to a Luau value (tables, strings, numbers, booleans; JSON `null` arrives as `nil`). `result` SHALL be `nil` when the agent completed the turn without any accepted submission.

#### Scenario: Submitted value returned
- **WHEN** the agent submits `{ "verdict": "approve", "score": 8 }` against the matching schema
- **THEN** the outcome's `result.verdict == "approve"` and `result.score == 8`

#### Scenario: Turn ends without submission
- **WHEN** the agent ends its turn having never called the submit tool
- **THEN** the outcome's `result` is `nil` and `stop_reason`/`text` are unaffected

### Requirement: In-turn validation with retry
Each submission SHALL be validated against the declared schema before acceptance. A submission that fails validation SHALL be reported to the agent as a failed tool result naming the violations, and the turn SHALL continue so the agent can correct the value and submit again. Only submissions that pass validation are accepted.

#### Scenario: Invalid then valid in one turn
- **WHEN** the agent submits a value missing a required property, receives the violation error, then submits a corrected value
- **THEN** the outcome's `result` is the corrected value

#### Scenario: Violations are actionable
- **WHEN** a submission violates the schema
- **THEN** the tool error text identifies the failing location and reason (for example, a missing required property by name)

### Requirement: Submission slot lifecycle
Accepted submissions SHALL follow last-wins semantics within a turn. The slot SHALL be cleared when a prompt starts, so a turn never observes the previous turn's value. A turn that ends cancelled (by `session:cancel()`) or times out SHALL discard any submission: `result` is `nil` regardless of what landed before cancellation. A submission arriving after the turn has settled SHALL be dropped and reported as one lifecycle log line, and SHALL NOT appear in any later outcome.

#### Scenario: Last submission wins
- **WHEN** the agent submits twice in one turn with valid values
- **THEN** the outcome's `result` is the second value

#### Scenario: Fresh slot per turn
- **WHEN** turn 1 submits a value and turn 2 ends without submitting
- **THEN** turn 2's `result` is `nil`

#### Scenario: Cancelled turn discards
- **WHEN** a valid submission lands and the script then cancels the in-flight turn
- **THEN** the outcome's `result` is `nil` and `stop_reason` is `cancelled`

### Requirement: Concurrent result sessions
Multiple result sessions SHALL operate independently: each session's schema, submissions, and outcomes are separate, and concurrent prompts on different sessions do not interfere.

#### Scenario: Two sessions, two schemas
- **WHEN** two result sessions with different schemas run prompts concurrently
- **THEN** each outcome's `result` validates against its own session's schema only

### Requirement: Graceful degradation
If the agent does not use the injected server — because it ignores the offered MCP servers, cannot access the ponos binary, or is sandboxed away from the transport — prompts SHALL still complete normally with `result` as `nil`, and ponos SHALL emit exactly one lifecycle diagnostic (a `[ponos]` line shown under `--verbose`) noting the session ran without typed results. Degradation SHALL NOT raise errors, hang turns, or change `text`/`stop_reason`.

#### Scenario: Agent ignores injected servers
- **WHEN** an agent completes a turn without ever connecting to the injected server
- **THEN** the turn completes with `result == nil` and one lifecycle log line under `--verbose`

#### Scenario: No hang on missing bridge
- **WHEN** the injected server cannot be spawned by the agent
- **THEN** the prompt turn still reaches completion

### Requirement: Local-only result transport
The channel between the injected server and ponos SHALL be a local Unix domain socket in a per-user temporary directory, created when the result session is created and removed when the session closes. The socket path SHALL NOT be guessable without access to the session's configuration, and the socket SHALL NOT accept connections after the session is closed.

#### Scenario: Socket lifecycle
- **WHEN** a result session is created and then closed
- **THEN** the per-session socket path exists only for the session's lifetime
