# Design: Prompt Text Is the Turn's Last Message

## Context

Turn folding lives in `src/acp/mod.rs`: `TurnFold` accumulates updates on the
connection's dispatch loop (wire order, before the response is delivered);
`run_turn` drains the fold once the prompt response lands. Today `text` is a
single `String` that every `agent_message_chunk` appends to, is only drained
on the success path, and is never reset per turn. See proposal.md for why that
is wrong.

## Goals / Non-Goals

**Goals**

- `r.text` = last agent message of the turn, per the scripting delta spec.
- Turn text is fully drained on every settle path (success, cancelled,
  timeout, transport error) — no cross-turn leakage.
- Tests can script multi-message turns offline via the mock agent.

**Non-Goals**

- Exposing intermediate messages to scripts (e.g. `r.messages` or a streaming
  callback). If a need emerges it becomes its own change.
- Changing renderer behavior: all chunks still stream as they arrive.
- Boundary detection beyond tool-call activity (see D1).

## Decisions

### D1: Message boundaries are tool-call activity only

A "message run" is a maximal sequence of `agent_message_chunk` updates not
interrupted by `tool_call` / `tool_call_update`. Tool use is the dominant
interleaving in real agents (narrate → act → answer) and can never split one
logical message, since an agent that emits a tool call has, by construction,
finished speaking.

**Alternatives considered**

- *Every non-message update is a boundary* (also plan, usage, config pushes):
  rejected — those updates can legitimately land between chunks of one
  streaming message (usage/config pushes are position-independent), and
  splitting there would silently truncate `r.text` to the tail of the final
  message. Tool activity is the only update class that provably separates
  messages.
- *No boundaries, take everything*: the status quo being replaced.

### D2: Fallback to the previous run when the turn ends on tool activity

`TurnFold` keeps `text` (current run) and `prev_text` (last completed
non-empty run). Settling yields `text` if non-empty, else `prev_text`, else
`""`. This covers turns that end on a tool call (e.g. `max_turn_requests`),
where the previous message is still "the last thing the agent said" and far
more useful to scripts than `""`.

**Alternative**: return `""` when there is no trailing message — rejected as
strictly less useful with no added safety.

### D3: Settling owns text; cancelled discards it

`settle_turn(discard)` becomes the single point that produces and drains text
(returns `(String, Option<submission>)`), replacing the `std::mem::take` at
the `run_turn` call site. `discard = true` — cancelled response, or the
error path (timeout / transport failure) — yields empty text, mirroring the
existing submission-discard rule. `begin_turn` also clears both fields so the
invariant "a turn never observes the previous turn's state" holds even if a
settle path is ever missed.

### D4: Mock fixture — `MOCK_LEAD_CHUNKS`

The mock streams `MOCK_CHUNKS` (or its echo modes) as the final message after
all tool/plan/usage emissions. New `MOCK_LEAD_CHUNKS` (`|`-separated, same
format) streams at turn start before tool activity, giving tests the
lead-message → tool → final-message shape. Setting `MOCK_CHUNKS` to an empty
value emits an empty final chunk, which exercises the D2 fallback (turn ends
on tool activity with no trailing message).

## Risks / Trade-offs

- [Scripts that (incorrectly) relied on concatenated preamble in `r.text`] →
  Pre-release experimental project, free to break the surface; README and
  spec updated in the same change.
- [An agent splits one logical message across a tool-call gap (unobserved in
  practice — tool use implies finished speaking)] → `r.text` would carry only
  the tail; acceptable and consistent with D1's rationale.
- [Cancelled turns now return `""` instead of partial text] → Deliberate
  (D3); the watchdog example asserts `stopReason`, not text.

## Migration Plan

Single PR: fold change, mock fixture, tests, README/spec. Rollback = revert.

## Open Questions

(none)
