## Why

Typed-result sessions currently append a fixed submit instruction to every
prompt (`RESULT_SUBMIT_INSTRUCTION` in `src/acp/mod.rs`). This silently mutates
the script author's prompt text, repeats identically every turn, and hardcodes
a Claude-Code-specific tool name (`mcp__ponos__result_submit`) that may mislead
other agents. The submit tool is already discoverable through normal MCP tool
listing and its own description, so the per-prompt nudge is redundant guidance
the script author did not ask for and cannot opt out of.

## What Changes

- Prompts on result sessions are sent **verbatim**: the fixed submit
  instruction is no longer appended to prompt text. **BREAKING** for scripts
  that relied on ponos telling the agent how to submit; script authors who
  want the instruction now write it in the prompt themselves (or let the agent
  discover the tool via its description).
- The `result_submit` tool description gains the timing the removed sentence
  carried ("call when your work is complete"), so guidance moves from
  per-prompt injection to per-tool metadata the agent consults on its own.
- No change to the contract mechanics: server injection, schema-wrapped
  `inputSchema`, validation, bounce-back on violations, `result = nil` when a
  turn ends without an accepted submission.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `typed-results`: the "Submit tool injection" requirement drops the clause
  mandating that every prompt carry a fixed submit instruction and rewrites
  the body of its "Prompt carries the instruction" scenario to assert the
  prompt passes through verbatim (the scenario header keeps its historical
  name — openspec deltas cannot rename scenario headers); the requirement
  keeps the server/tool/schema injection and adds that the tool description
  carries the submit guidance.

## Impact

- `src/acp/mod.rs` — delete `RESULT_SUBMIT_INSTRUCTION` and the append in the
  turn path; the `result_contract` flag on that path likely becomes unused.
- `src/bridge.rs` — one-line tool description update in `tool_for`.
- `tests/typed_results.rs` — replace `prompt_on_result_session_carries_submit_instruction`
  with the inverse assertion (prompt text arrives verbatim, no suffix).
- `README.md` — the typed-results bullet listing "appends one fixed sentence
  to each prompt" is updated to describe verbatim prompts and the
  tool-description guidance.
- Spec sync: `openspec/specs/typed-results/spec.md` requirement updated via
  this change's delta.
