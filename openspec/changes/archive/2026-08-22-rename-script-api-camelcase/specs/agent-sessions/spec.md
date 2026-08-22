## MODIFIED Requirements

### Requirement: Prompt turns drive the full update stream
During a prompt turn ponos SHALL receive `session/update` notifications and: accumulate `agent_message_chunk` into the final message text, track tool call and plan updates for display, and render `usage_update` context-window information for display. Token counts SHALL be taken from the `session/prompt` response and returned as the result's `usage` (zero when the response reports none). The turn SHALL complete when the `session/prompt` response arrives, returning its `stopReason`.

#### Scenario: Chunks assemble final text
- **WHEN** an agent streams two `agent_message_chunk` updates ("Hel", "lo") and ends its turn
- **THEN** the prompt result's `text` is `"Hello"`

#### Scenario: Usage accounting
- **WHEN** an agent reports token usage on its `session/prompt` response
- **THEN** the result's `usage` reflects the reported token counts

#### Scenario: Context-window usage rendered
- **WHEN** an agent emits a `usage_update` during a turn
- **THEN** the reported context-window usage is rendered with session attribution

### Requirement: Cancellation maps to session/cancel
Calling `session:cancel()` during an in-flight turn SHALL send the `session/cancel` notification to that session's agent, and `timeoutMs` expiry on `prompt` SHALL do the same before raising the timeout error.

#### Scenario: Timeout cancels remotely
- **WHEN** a prompt exceeds `timeoutMs`
- **THEN** ponos sends `session/cancel` and raises the timeout error to the script
