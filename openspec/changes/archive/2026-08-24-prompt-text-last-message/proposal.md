# Prompt Text Is the Turn's Last Message

## Why

`session:prompt` concatenates every `agent_message_chunk` of the turn into
`r.text`. A real agent that narrates ("Let me check that file…"), runs tools,
then answers ("The bug is on line 3") yields a preamble-glued blob that no
script wants — every bundled example treats `tostring(r)` as "the answer". The
scripting spec already promises "`r.text` is the agent's final message"; the
implementation just doesn't deliver it. Two adjacent defects make it worse:
partial text from a cancelled/timed-out/failed turn leaks into the next turn's
`r.text` (the fold is never drained on the error path), and the mock agent can
only stream a single contiguous message, so the suite cannot tell the two
semantics apart.

## What Changes

- `r.text` becomes the turn's **last agent message**: the final contiguous run
  of `agent_message_chunk` text, where tool-call activity (`tool_call`,
  `tool_call_update`) ends the current message run. Earlier messages are
  dropped from `r.text` (they remain visible in the streaming renderer, which
  is unchanged).
- If a turn ends with no message after its last tool activity (e.g. it ends on
  a tool call), `r.text` falls back to the previous non-empty message run of
  that turn; a turn with no agent message at all yields `""`.
- A turn that completes with `stopReason = "cancelled"` returns `r.text == ""`
  (a cancelled turn's partial text is as unreliable as its discarded
  submission).
- **BREAKING (fix)** — `r.text` no longer contains preamble messages glued
  before the final one, and cancelled turns no longer return partial text.
- **BUGFIX** — turn text is always drained at turn settle, so a
  cancelled/timed-out/failed turn can no longer prefix the next turn's
  `r.text` on the same session.
- The mock agent gains `MOCK_LEAD_CHUNKS`: `|`-separated chunks streamed at
  turn start, before tool activity, so tests can script the
  message → tool → message interleaving.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `scripting`: "Prompt returns a result table" requirement — `text` is
  redefined as the turn's last agent message (tool-call boundaries, fallback
  when the turn ends on tool activity, empty for cancelled turns and
  message-less turns); new scenarios for last-message selection, fallback, and
  no cross-turn leakage.

## Impact

- `src/acp/mod.rs` — `TurnFold` tracks the current message run plus the last
  completed run; tool-call updates break the run; `settle_turn` returns and
  drains text (success, cancelled, and error paths); `begin_turn` resets it.
- `src/bin/mock-agent/main.rs` — new `MOCK_LEAD_CHUNKS` emission before tool
  activity.
- `tests/acp.rs` — turn-outcome tests for last-message semantics; unit tests
  in `src/acp/mod.rs` for the fold (boundaries, fallback, discard, no leak).
- `README.md` — prompt-result table row and prose for `text`.
- No script-surface or type-definition changes: `r.text` stays a string.
