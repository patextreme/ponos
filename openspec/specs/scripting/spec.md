# Scripting Specification

## Purpose

Defines the Luau scripting environment embedded in ponos: the sandboxed standard library, module resolution, the `ponos` API namespace, concurrency primitives, and error/cancellation semantics observable by script authors.

## Requirements

### Requirement: Sandboxed Luau environment
Scripts SHALL execute in a sandboxed Luau environment exposing only: `string`, `table`, `math`, `utf8`, `bit32`, `buffer`, `os.time`, `os.clock`, `print`, and a restricted `coroutine` table containing only `yield` (retained because the embedded async runtime requires it; all coroutine scheduling primitives MUST remain absent). The environment MUST NOT expose file I/O, subprocess execution, network, or debug facilities. Scripts have no host filesystem or network access beyond driving agents.

#### Scenario: Sandboxed globals
- **WHEN** a script accesses `io`, `os.execute`, `debug`, or `coroutine.create`
- **THEN** the access resolves to nil (or raises an error on call) because the globals are absent

#### Scenario: Print passthrough
- **WHEN** a script calls `print("hello")`
- **THEN** the line is written to ponos's standard output unmodified, without session prefixes

### Requirement: Relative module resolution
Scripts SHALL be able to `require` modules by relative path from the requiring file's directory (e.g. `require("./lib/pipeline")`, resolving `.luau` files). Absolute paths or paths escaping the script tree MUST be rejected with a Lua error.

#### Scenario: Sibling module
- **WHEN** a script at `main.luau` requires `./lib/util` and `lib/util.luau` exists
- **THEN** the module is loaded and its return value provided; a second require of the same path returns the cached module

#### Scenario: Missing module
- **WHEN** a script requires a path that does not resolve to an existing `.luau` file
- **THEN** the require call raises a Lua error naming the unresolved path

### Requirement: Agent and session API
The `ponos` namespace SHALL provide `ponos.agent(name_or_spec)` returning an agent factory, and `agent:session(options)` returning a session object. Each `session()` call creates an independent session with its own agent subprocess. Session options SHALL accept `cwd` (resolved relative to the invocation directory), `id` (label used in output attribution, defaulting to `s1`, `s2`, … per agent), `mcpServers`, `resultSchema` (a JSON Schema expressed as a Luau table; the option's semantics are specified by the typed-results capability), and `config` (a Luau table mapping config-option ids to string or boolean values; the option's semantics are specified by the session-config-options capability). Two `ponos.agent` calls for the same name SHALL return independent factory objects.

#### Scenario: Session creation
- **WHEN** a script calls `ponos.agent("claude"):session({ id = "reviewer" })`
- **THEN** a session labeled `claude/reviewer` exists and is ready to prompt

#### Scenario: Default session labels
- **WHEN** two sessions are created without `id` from the same agent factory
- **THEN** they are labeled `s1` and `s2` respectively in output attribution

#### Scenario: Independent factories
- **WHEN** `ponos.agent("claude")` is called twice with the same name and each factory creates a session
- **THEN** the factories keep independent session counters: both first sessions are labeled `claude/s1`

#### Scenario: Unknown agent name
- **WHEN** `ponos.agent("nope")` is called and `nope` exists in no registry
- **THEN** a Lua error is raised naming the unresolved agent

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

### Requirement: Cancellation is control flow, not failure
`session:cancel()` SHALL be callable while another task is blocked in `prompt` on that session; it sends `session/cancel`, and the awaiting `prompt` returns normally with `stopReason = "cancelled"` rather than raising.

#### Scenario: Watchdog cancel
- **WHEN** task A is blocked in `s:prompt(...)` and task B calls `s:cancel()`
- **THEN** task A's `prompt` returns a result with `stopReason == "cancelled"` and no error is raised

### Requirement: Task and concurrency primitives
The `ponos` namespace SHALL provide: `ponos.spawn(fn)` returning a Task object with `:await()`, `ponos.join({task, ...})` waiting for all tasks, `ponos.parallel(items, fn, options?)` running `fn` per item with optional `concurrency` limit (default unlimited) and returning per-item outcome entries, and `ponos.sleep(ms)`. Awaiting an errored task SHALL re-raise its error at the await site. `ponos.parallel` results SHALL carry each item's success value or error without throwing wholesale. A task error is delivered when observed via `:await()`, `join`, or carried in `ponos.parallel` results, whether or not the script catches or inspects it; a task error never delivered by script end SHALL fail the run (error to stderr, non-zero exit).

#### Scenario: Parallel fan-out
- **WHEN** `ponos.parallel({1,2,3}, function(i) return s:prompt("q"..i) end)` runs
- **THEN** all three prompts execute concurrently and results arrive in item order

#### Scenario: Concurrency cap
- **WHEN** `ponos.parallel(items, fn, { concurrency = 2 })` runs with 5 items
- **THEN** at most 2 `fn` invocations are in flight simultaneously

#### Scenario: Contained task error
- **WHEN** one of three spawned tasks raises and the script joins all three
- **THEN** the two successful values are available and the failed task's error is reported for its entry; other tasks are unaffected

#### Scenario: Error re-raised at await
- **WHEN** a spawned function raises an error and `task:await()` is called
- **THEN** the original error is re-raised at the await call site

### Requirement: Runtime helpers
The `ponos` namespace SHALL provide `ponos.log(msg)` printing a `[ponos]`-prefixed diagnostic line to standard output, `ponos.exit(code)` terminating the run, `ponos.sleep(ms)` yielding the current task for the duration, and `ponos.version` (read-only version string).

#### Scenario: Log attribution
- **WHEN** a script calls `ponos.log("starting")`
- **THEN** output shows `[ponos] starting` on its own line

#### Scenario: Sleep yields
- **WHEN** task A calls `ponos.sleep(100)` while task B prompts
- **THEN** task B progresses during A's sleep
