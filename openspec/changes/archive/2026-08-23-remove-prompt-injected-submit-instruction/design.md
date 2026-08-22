# Design: remove prompt-injected submit instruction

## Context

Typed-result sessions append `RESULT_SUBMIT_INSTRUCTION` (`src/acp/mod.rs:56`)
to every prompt inside `run_turn`, gated on a `result_contract: bool` threaded
from the driver (`opts.result.is_some()` at `src/acp/mod.rs:679`). The
`result_submit` tool description (`src/bridge.rs`, `tool_for`) already tells
the agent what the tool is for; the per-prompt append duplicates that guidance
in prompt text the script author did not write. See proposal.md - Why.

## Goals / Non-Goals

Goals:
- Prompt text on result sessions reaches the agent byte-identical to what the
  script passed.
- Submit guidance survives through tool metadata, so agents that consult tool
  listings still learn when and how to submit.
- Delete the append path and its now-dead flag threading.

Non-Goals:
- Any opt-in mechanism for the old instruction (no `ponos`-side toggle). Script
  authors who want the sentence write it in their prompt.
- Changes to submission mechanics: server injection, schema wrapping,
  validation bounce-back, last-wins within a turn, `result = nil` semantics.

## Decisions

### 1. Guidance moves to the `result_submit` tool description

Update `tool_for`'s description to carry the timing the sentence had, e.g.
"Call this when your work on the task is complete, with the final result as
the `value` argument; `value` must satisfy the session's declared JSON Schema;
violations are reported back so you can correct the value and submit again."

Rationale: tool descriptions are the channel agents natively consult, are
declared once per tool rather than repeated per prompt, and are portable — no
hardcoded `mcp__ponos__result_submit` naming (that string is Claude Code's MCP
tool-naming convention; other agents derive different names). Alternative
considered: moving the instruction into the session's `mcpServers` entry or
environment — rejected, those are not surfaces agents read as guidance.

### 2. Delete the `result_contract` flag on the turn path

The flag exists solely to gate the append (param at `run_turn`, threading at
the driver's `Prompt` arm). Delete the param, the local, and the threading.
Session-level contract state (the result channel, submission folding) is
independent of this flag and stays untouched.

### 3. Keep the no-submission failure mode as-is

A turn that ends without an accepted submission still yields `result = nil`
(and the documented never-observed-task-error path when a script treats that
as failure). This is the honest observable outcome; documenting that authors
may need to mention submission in their prompt is a README concern, not a
mechanism change.

## Risks / Trade-offs

- [Agents less reliably submit without the per-prompt nudge → more turns end
  with `result = nil`] → Mitigation: the strengthened tool description carries
  the when/how; README documents that authors can mention submission in the
  prompt for reluctant agents. Acceptable: failure is observable and
  script-side, never silent corruption.
- [Scripts depending on the appended sentence regress] → Mitigation: BREAKING
  is flagged in the proposal; the sentence is short enough to copy into a
  prompt verbatim if needed.
- [Prompt-verbatim test could pass trivially if the mock echoes text it never
  received] → Mitigation: existing mock agent echoes the exact prompt text
  (`MOCK_NO_MCP` pattern in `tests/typed_results.rs`), so the flipped
  assertion exercises the real wire text.

## Migration Plan

Single release: delete the append, update the description, flip the test,
update README. Rollback is trivially the inverse patch; no persisted state is
affected.
