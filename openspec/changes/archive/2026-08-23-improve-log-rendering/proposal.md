## Why

ponos's streaming log output is noisy and low-signal: tool calls repeat themselves and flood the log, `tool_call_update` lines render a raw call id (`tool: call_0bb9d326dd5c444e92bff218 (in_progress)`) that means nothing to a human, and lines carry no timestamp, so it is impossible to tell when anything happened or how long anything took.

## What Changes

- Every rendered line (message chunks, tool lines, plan, usage, lifecycle diagnostics, `ponos.log`, `-vv` stderr passthrough) gets a wall-clock local-time `HH:MM:SS` timestamp prefix. Always on; no new flag. `--quiet` behavior is unchanged; `--no-color` keeps the timestamp as plain text.
- Tool call rendering becomes title-based and transition-aware:
  - ponos keeps a per-session map of tool call id → title from `tool_call` notifications, so update lines show the tool title, never a bare call id (raw id remains the fallback when an update precedes its announcement).
  - Exactly two lines per call in the common case: one when the call enters `in_progress` (bare title, no status suffix), one when it settles (`completed` / `failed` / cancelled-equivalent terminal states), e.g. `tool: Search files "foo" (completed, 3.2s)`.
  - `pending` renders nothing. Repeated identical status updates for the same call render nothing. This kills both flood sources: agents that resend the same status, and the id-only update lines.
- Terminal tool lines include wall-clock duration measured from the call's first observed activity (its `in_progress` transition, or first observation when no `in_progress` ever arrives).

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `agent-sessions`: the "Prompt turns drive the full update stream" requirement gains the tool-line rendering contract — titles resolved via the id→title map, start + terminal transitions only, deduped repeats, `pending` silent.
- `cli`: the "Output control flags" requirement gains the always-on timestamped line format for all rendered output.

## Impact

- `src/render/mod.rs`: timestamp acquisition/formatting, dimmed style under color, plumbed through `line`/`chunk`/`event`/`agent_stderr`/`lifecycle`/`script_log`.
- `src/acp/mod.rs` (`fold_update`): per-session tool call state (id → title, first-activity instant, last-seen status), new `DisplayEvent::Tool` payload shape.
- `src/bin/mock-agent/`: a way to script an `in_progress` status (and repeated updates) so the dedup/start-line behavior is testable offline.
- `tests/` (`acp.rs`, `e2e.rs`, `examples.rs`): assertions on rendered lines (timestamp presence, single tool line, no raw ids).
- `README.md`: output-format documentation.
- No script-API, config, or protocol-surface changes; exit codes untouched.
