## MODIFIED Requirements

### Requirement: Submit tool injection
When `result` is set, session creation SHALL offer the agent one additional MCP server over stdio, named `ptah`, exposing exactly one tool named `result_submit`. The tool's input schema SHALL be the declared schema wrapped under a single `value` property, so the declared schema reaches the model through the tool itself. The tool's description SHALL tell the agent to call it with its final result as `value` when its work is complete. Prompt text on a result session SHALL be passed through verbatim: ptah SHALL NOT append instructions to, or otherwise modify, the script's prompt. The schema SHALL NOT be inlined into prompt text. The injected server SHALL NOT change sessions that declare no `result`.

#### Scenario: Tool appears with wrapped schema
- **WHEN** a result session's agent lists tools from the injected server
- **THEN** exactly one tool `result_submit` is listed, whose input schema is `{ value: <declared schema> }`

#### Scenario: Tool description carries the submit guidance
- **WHEN** a result session's agent lists tools from the injected server
- **THEN** the `result_submit` tool's description instructs the agent to call it when its work is complete, with the final result as the `value` argument

#### Scenario: Prompt carries the instruction
- **WHEN** a prompt is sent on a result session
- **THEN** the text the agent receives is identical to the prompt text the script passed, carrying no ptah-appended instruction or suffix

### Requirement: Graceful degradation
If the agent does not use the injected server — because it ignores the offered MCP servers, cannot access the ptah binary, or is sandboxed away from the transport — prompts SHALL still complete normally with `result` as `nil`, and ptah SHALL emit exactly one lifecycle diagnostic (a `[ptah]` line shown under `--verbose`) noting the session ran without typed results. Degradation SHALL NOT raise errors, hang turns, or change `text`/`stopReason`.

#### Scenario: Agent ignores injected servers
- **WHEN** an agent completes a turn without ever connecting to the injected server
- **THEN** the turn completes with `result == nil` and one lifecycle log line under `--verbose`

#### Scenario: No hang on missing bridge
- **WHEN** the injected server cannot be spawned by the agent
- **THEN** the prompt turn still reaches completion

### Requirement: Local-only result transport
The channel between the injected server and ptah SHALL be a local Unix domain socket in a per-user temporary directory, created when the result session is created and removed when the session closes. The socket path SHALL NOT be guessable without access to the session's configuration, and the socket SHALL NOT accept connections after the session is closed.

#### Scenario: Socket lifecycle
- **WHEN** a result session is created and then closed
- **THEN** the per-session socket path exists only for the session's lifetime
