## Context

Rendering today lives in `src/render/mod.rs`: a `Renderer` wraps a `BufWriter<Stdout>` behind a `Mutex` and emits `[label] line` strings, with per-label ANSI colors cycled from a fixed palette. `src/acp/mod.rs::fold_update` translates each `session/update` into a `DisplayEvent` and forwards it; tool calls currently emit `tool: <title-or-raw-id> (<status>)` for every `tool_call` and every `tool_call_update` that carries a status, which is where the flood and the `call_0bb9…` ids come from. Timestamps exist nowhere. Tests drive the in-repo mock agent (`src/bin/mock-agent/`), whose `MOCK_TOOL` scripts `pending → completed` (no `in_progress`, no repeats).

## Goals / Non-Goals

**Goals:**
- Timestamps on every renderer-emitted line, uniform across all line kinds, with zero new CLI surface.
- Tool log lines that a human can read: title, meaningful transitions only, duration on settlement.
- Keep all behavior offline-testable through the mock agent.

**Non-Goals:**
- Any change to the ACP protocol surface, script API, exit codes, or config.
- Dedup for plan or usage lines (only tool lines were reported as flooding).
- ISO-8601 dates, timezone labels, or sub-second precision in timestamps (a log line's `HH:MM:SS` is sufficient; duration carries the sub-second signal for tools).

## Decisions

### 1. Timestamps are taken and formatted at write time, inside `Renderer`

`prefixed_line` (the single choke point every renderer line already passes through) prepends the timestamp, so `line`, `chunk`, `event`, `agent_stderr`, `lifecycle`, and `script_log` get it for free and cannot forget it. Format: local wall-clock `HH:MM:SS` 24-hour via a small time formatting step; when color is on the timestamp is dimmed (`\x1b[2m`), under `--no-color` it is plain text.

- *Alternatives considered*: a `--timestamps` flag (rejected in grilling — always on); elapsed-since-start counters (rejected in grilling — wall clock chosen; elapsed time for tools arrives via durations); threading a timestamp through each `DisplayEvent` (rejected — the event producers have no meaningful "event time" distinct from render time, and it duplicates the choke point).
- *Dependency choice*: `jiff` with `default-features = false, features = ["tz-system"]` — no bundled IANA tzdb (it reads the system zone via `/etc/localtime`/`TZ`), pure Rust, no unsafe, wrapped behind one `hhmmss() -> String` helper so the choice stays swappable. Alternative rejected: hand-rolling `libc::localtime_r` (libc is already a dep and the codebase is unix-gated, but unsafe `struct tm` math to save one crate is a bad trade); also rejected: `chrono` (heavier, `localtime` handling less ergonomic).

### 2. Tool call state lives in the renderer-adjacent fold, keyed per session

`fold_update` already holds per-session state (`TurnFold`). Tool call display state (id → title, first-activity `Instant`, last-rendered status) is a `HashMap` alongside it in the driver, not inside `Renderer`:
- The renderer stays a dumb sink; policy (what deserves a line) stays where updates arrive.
- State is bounded by the number of tool calls in a session; entries can be retained for the session lifetime (a repeat terminal status for an old id must still dedup). No eviction needed at expected volumes.

Line shapes: start `tool: <title>`, terminal `tool: <title> (<status>, <dur>)` where `<dur>` is `X.Ys` under a minute and `Mm SS.Ss` above it. Status strings reuse `status_string` (`completed`/`failed`). A `tool_call` arriving already `in_progress` renders the start line immediately; `pending` announcements only seed the map.

### 3. `DisplayEvent::Tool` carries the final display string, not raw fields

The transition policy is decided in `fold_update` (where state lives); the event becomes `Tool(String)` — the fully formatted line body. This keeps the enum a display contract and avoids the renderer needing to know statuses. Alternative rejected: carrying `{title, status, duration}` and formatting in the renderer — no second consumer exists to justify it.

### 4. Mock agent extension for offline coverage

`MOCK_TOOL` keeps its current shape (compat with existing tests) and a new mode, `MOCK_TOOL_FLOW` (comma-separated statuses, e.g. `pending,in_progress,in_progress,completed`), replays an arbitrary status sequence: a `tool_call` for the first entry then `tool_call_update`s for the rest, plus a titled announcement so id→title resolution is exercised. This is the fixture for the dedup, start-line, and duration scenarios. AGENTS.md's rule — extend the mock, never a real agent — is why this is a design-level decision.

### 5. Delta spec placement

Tool-line rendering is `agent-sessions` (the requirement that already owns streaming update handling); the timestamp format is `cli` (the capability that owns output control). A new `rendering` capability was considered and rejected: no existing spec boundary owns renderer internals, and two one-requirement deltas beat a new spec whose purpose overlaps both.

## Risks / Trade-offs

- [Timestamps make lines wider and slightly noisier for interactive use] → Chosen explicitly in grilling (always on); dimming under color keeps them visually secondary. `--quiet` users see nothing.
- [Test churn: every test that asserts on rendered output now has a timestamp prefix] → Assertions must match with a wildcard prefix or the harness should strip a leading `\d{2}:\d{2}:\d{2} ` before comparing; one helper in the test crate keeps this uniform.
- [Agents that legitimately change status back and forth (`in_progress` → `pending` → `in_progress`) would re-print] → Dedup compares against last-rendered status only, so a genuine transition re-renders; that is signal, not flood.
- [New `jiff` dependency widens the build] → Configured without the bundled tzdb (`tz-system` only), pure Rust, no codegen; isolated behind one helper function.
- [Duration for a call that never reaches a terminal status (turn cancelled mid-tool)] → No terminal line is rendered; the state entry simply dies with the session. Cancelled turns already discard their fold.

## Migration Plan

Single-step change to a CLI tool's display output; no persisted state, protocol, or API migration. Rollback is `git revert`. Consumers parsing ponos stdout (none known beyond humans and tests) see a new leading timestamp token and renamed tool-line bodies — acceptable for a pre-1.0 tool per README positioning.
