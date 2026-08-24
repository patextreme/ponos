## MODIFIED Requirements

### Requirement: Prompt returns a result table
`session:prompt(text, options?)` SHALL send one prompt turn and return a table with `text` (the turn's last agent message), `stopReason` (`"end_turn"`, `"max_tokens"`, `"max_turn_requests"`, `"refusal"`, or `"cancelled"`), `usage` (`input`, `cacheRead`, `cacheWrite`, `output` token counts, zero when unreported), and `result` (the turn's last accepted typed submission converted to a Luau value; `nil` when the session declared no contract or the turn had no accepted submission — the field's semantics are specified by the typed-results capability). The result table SHALL be directly string-coercible to `text` via `__tostring`. Options SHALL accept `timeoutMs`.

`text` SHALL be the last agent message of the turn: the final contiguous run of agent message text, where tool-call activity (`tool_call` and `tool_call_update` updates) terminates the current message run. When a turn ends with no message after its last tool-call activity, `text` SHALL fall back to the previous non-empty message run of that turn; when a turn produces no agent message at all, `text` SHALL be the empty string. When a turn completes with `stopReason == "cancelled"`, `text` SHALL be the empty string. Text from one turn SHALL never appear in a subsequent turn's `text` on the same session, whatever the previous turn's outcome. Streaming display of intermediate messages is unaffected: every message chunk is still surfaced by the live renderer as it arrives.

#### Scenario: Successful turn
- **WHEN** `local r = s:prompt("hi")` completes normally
- **THEN** `r.text` is the agent's final message, `tostring(r)` equals `r.text`, and `r.stopReason == "end_turn"`

#### Scenario: Last message after tool use
- **WHEN** a turn streams message A, then tool-call activity, then message B, and the turn completes
- **THEN** `r.text` equals message B and does not contain message A, while the run's streaming output showed both messages

#### Scenario: Turn ends on tool activity
- **WHEN** a turn streams message A, then tool-call activity, and completes with no message after it
- **THEN** `r.text` equals message A

#### Scenario: Cancelled turn has empty text
- **WHEN** a turn streams partial message text and is then cancelled (`stopReason == "cancelled"`)
- **THEN** `r.text` is the empty string

#### Scenario: No text leaks across turns
- **WHEN** a turn times out or is cancelled after streaming partial text, and the next prompt turn on the same session completes with message B
- **THEN** the next turn's `r.text` equals message B exactly, with no prefix from the aborted turn

#### Scenario: Timeout is an error
- **WHEN** `s:prompt("...", { timeoutMs = 50 })` exceeds its timeout
- **THEN** the turn is cancelled via `session/cancel` and the call raises a catchable Lua timeout error
