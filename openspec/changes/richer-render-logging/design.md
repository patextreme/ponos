## Context

The renderer (`src/render/mod.rs`) is a dumb sink: `fold_update` in
`src/acp/mod.rs` decides which lines deserve rendering and hands down
fully formatted bodies via `DisplayEvent`. That split is sound and stays:
all new policy (peek selection, truncation, path shortening) belongs in
the ACP fold layer; the renderer only learns the new timestamp shape and,
at most, a new `DisplayEvent` variant for the prompt line. Key facts from
research:

- ACP v1 `ToolCall` already carries `kind`, `locations: Vec<{path, line?}>`,
  `raw_input: Option<Value>`, `raw_output`; `ToolCallUpdateFields` carries
  `kind`, `locations`, `raw_input`, `title` alongside `status`. ponos
  currently parses only id/title/status.
- `run_turn` (`src/acp/mod.rs`) already receives the prompt text and
  renderer/label (params `_renderer`/`_label` are unused today).
- pi-acp: non-bash titles are bare tool names; bash titles are already the
  full command; locations are absolute paths; `rawInput` is the args
  object. opencode's emission is unverified — the generic fallback path
  must carry the design when fields are absent.
- Session cwd is known at `session/new` and threads into `SessionHandle`
  state but not into the `ToolFold` today.

## Goals / Non-Goals

**Goals:**

- One prompt line per turn and one peek per tool line, both default-on,
  quiet-gated, honoring the existing flood guards.
- A single shared truncation budget (~120 visible chars, `…` suffix).
- Path shortening against the session cwd and `$HOME`.
- Full test coverage offline via the mock agent (new env knobs).

**Non-Goals:**

- Output-side peeks (bash output, exit codes) — input only.
- A durable log file or JSONL transcript; `--verbose` semantics unchanged.
- Lifting truncation under `-v`.
- Touching the Luau API or exit codes.

## Decisions

### D1: Prompt line rendered in `run_turn`, not the bridge

`run_turn` owns the send and already holds `text`, `renderer`, `label`; the
unused params become used ones. Collapsing (runs of whitespace → single
space) and budgeting happen inline at send time — no state needed.
*Alternative*: render in `bridge.rs` where `session:prompt` is invoked —
rejected: it would duplicate the truncation helper or pass prepped text
through the turn API.

### D2: Peek synthesis lives in `ToolFold`, stored per call

`ToolCallDisplay` gains a `peek: Option<String>` computed at fold time from
`kind`/`locations`/`raw_input` (announcement or update — first non-empty
candidate wins, later data does not overwrite an already-set peek; a title
learned late applies the containment check at render). Selection order:
`execute` → command/cmd string from `raw_input`; other known kinds →
shortened `locations[0]` as `path[:line]`; fallback → compact
`serde_json::to_string(raw_input)`. Containment check
(`title.contains(peek)`) suppresses duplication (pi-acp bash). Peek applies
to both start and terminal lines via the existing `transition()` formatter.
*Alternative*: compute peeks in the renderer from a richer `DisplayEvent`
payload — rejected: the renderer stays a dumb sink and the policy is
protocol-shaped.

### D3: Session cwd plumbed as `Arc<PathBuf>` into the fold state

Path shortening is pure (`strip_prefix(cwd)` → else home collapse → else
as-is) and needs only the session cwd; a helper module fn (unit-testable)
takes `(path, cwd, home)`. `ToolFold` gains the cwd at construction.
*Alternative*: shorten in the renderer — rejected: labels are strings
there, no path context.

### D4: Truncation counts visible chars, one constant

`const PEEK_BUDGET: usize = 120;` in the fold layer (or a small shared
`render::util`), applied to prompt lines and peeks, hard cut + `…`, ANSI
never present in the payload so a byte/char cut at `char_indices` suffices.
Compact JSON uses `serde_json::to_string` (no spaces).

### D5: Timestamp via `jiff` local datetime

`hhmmss()` becomes `timestamp()` returning
`yyyy-mm-dd HH:MM:SS` (`now.get_year()`-etc. or format string). Fixed width
for all lines; no day-rollover state machine.

### D6: Mock agent knobs

- `MOCK_PROMPT_ECHO` not needed (ponos renders the prompt itself).
- `MOCK_TOOL_KIND` (default `other`), `MOCK_TOOL_LOCATIONS` (`/abs/path[:line]`,
  comma-separated), `MOCK_TOOL_RAW_INPUT` (JSON) — attached to the existing
  `MOCK_TOOL` / `MOCK_TOOL_FLOW` emissions so every peek path is drivable.

## Risks / Trade-offs

- [Agents send `raw_input` on updates only (never at announce)] → peek is
  sticky-first-wins; a call whose input only appears later renders its
  later lines with the late-learned peek (title map already works this
  way).
- [Compact JSON of large inputs floods width] → the 120-char budget caps
  it; output-side data is out of scope.
- [Timestamp width costs 11 chars] → accepted in Q10; readability over
  width.
- [`contains` check false-negatives on quoted titles] → acceptable: worst
  case a duplicated short peek, not incorrect data.
- [Breaking change to line format] → tests and README updated in the same
  change; no external consumers beyond humans reading logs.

## Migration Plan

Single change lands atomically: code, mock knobs, tests, README, spec
deltas. Rollback = revert the change; no persistent state involved.
