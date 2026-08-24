## MODIFIED Requirements

### Requirement: Typed prompt outcomes
The table returned by `session:prompt()` on a result session SHALL include a `result` field: the last accepted submission for that turn, converted from JSON to a Luau value (tables, strings, numbers, booleans; JSON `null` arrives as `nil`). `result` SHALL be `nil` when the agent completed the turn without any accepted submission.

#### Scenario: Submitted value returned
- **WHEN** the agent submits `{ "verdict": "approve", "score": 8 }` against the matching schema
- **THEN** the outcome's `result.verdict == "approve"` and `result.score == 8`

#### Scenario: Turn ends without submission
- **WHEN** the agent ends its turn having never called the submit tool
- **THEN** the outcome's `result` is `nil` and `stopReason`/`text` are unaffected

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
- **THEN** the outcome's `result` is `nil` and `stopReason` is `cancelled`

### Requirement: Graceful degradation
If the agent does not use the injected server — because it ignores the offered MCP servers, cannot access the ponos binary, or is sandboxed away from the transport — prompts SHALL still complete normally with `result` as `nil`, and ponos SHALL emit exactly one lifecycle diagnostic (a `[ponos]` line shown under `--verbose`) noting the session ran without typed results. Degradation SHALL NOT raise errors, hang turns, or change `text`/`stopReason`.

#### Scenario: Agent ignores injected servers
- **WHEN** an agent completes a turn without ever connecting to the injected server
- **THEN** the turn completes with `result == nil` and one lifecycle log line under `--verbose`

#### Scenario: No hang on missing bridge
- **WHEN** the injected server cannot be spawned by the agent
- **THEN** the prompt turn still reaches completion
