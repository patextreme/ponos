## Why

Streaming render output is the only log ponos produces, and it hides the two
things an operator most wants to see: which prompt each turn is processing
(the prompt text is in hand at `run_turn` and never rendered), and what a
terse tool line like `tool: read` or `tool: bash` is actually doing (the ACP
`tool_call` carries `kind`, `locations`, and `raw_input`; ponos reads only id,
title, and status and drops the rest). In a multi-agent fan-out the log is
near-useless for follow-along debugging. All the data needed already arrives
over the wire and is discarded.

## What Changes

- Every prompt turn renders one `prompt:` line at send time: the prompt text
  collapsed to one line and truncated to a ~120 visible-char budget with `…`.
  Default-on for all sessions, suppressed by `--quiet`.
- Tool lines gain a kind-aware input peek appended after the title when the
  title does not already contain it, on both the start and terminal line:
  - `execute` kind → the `command`/`cmd` string from `raw_input`;
  - `read`/`edit`/`move`/`search` kinds → `locations[0]` as `path[:line]`;
  - otherwise → compact JSON of `raw_input` (same ~120-char budget).
- Paths in peeks render relative to the session's cwd when under it,
  `~`-collapsed when under the user's home, else absolute as received.
- Rendered line timestamps change from `HH:MM:SS` to
  `yyyy-mm-dd HH:MM:SS` (space-separated, local time) on every line.
- **BREAKING** (output format only): timestamp width changes, and tool lines
  can carry appended peeks. Exit codes and flag semantics are unchanged.
- Mock agent gains env knobs to emit `locations`/`raw_input` so peek paths
  are testable offline.

## Capabilities

### New Capabilities

- `render-logging`: the streaming stdout log's observability contract —
  prompt-line rendering, tool input peeks, path shortening, timestamp shape,
  truncation budget, and flag gating.

### Modified Capabilities

- `agent-sessions`: the tool-line contract (currently "title with no status
  suffix" start line, title + status + duration terminal line) now allows an
  input peek appended after the title, and requires that prompt sends render
  a prompt line. Scenario-level wording for start/terminal lines and flood
  guards stays, with the peek folded in.
- `cli`: the "Rendered lines are timestamped" requirement's shape changes
  from `HH:MM:SS` to `yyyy-mm-dd HH:MM:SS`, delegating the shape to the
  `render-logging` capability (single source of truth). Spec-only delta:
  the flag semantics it governs (always-on, `--no-color` plain text,
  `--quiet` suppression, `print` untouched) are unchanged.

## Impact

- `src/render/mod.rs` — timestamp format, possible `DisplayEvent` additions
  (prompt line), truncation helper.
- `src/acp/mod.rs` — `run_turn` renders the prompt line (currently unused
  `_renderer`/`_label` params); `fold_update`/`ToolFold`/`ToolCallDisplay`
  parse `kind`, `locations`, `raw_input` from `tool_call`/
  `tool_call_update` and synthesize peeks; session cwd plumbing from
  `session/new` into the fold for path shortening.
- `src/bin/mock-agent/` — new env vars (e.g. `MOCK_TOOL_LOCATIONS`,
  `MOCK_TOOL_RAW_INPUT`) to exercise peek paths.
- `tests/` — `tests/cli.rs`, `tests/typed_results.rs` line assertions gain
  dates/peeks; new tests for prompt lines, peek kinds, dedup, truncation,
  path shortening.
- `README.md` — Output format section rewritten to match.
- `openspec/` — new `specs/cli/` delta (timestamp shape, delegated to
  `render-logging`) alongside the `agent-sessions` delta; no code impact.
- No dependency changes; no Luau API changes.
