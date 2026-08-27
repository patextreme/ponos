## MODIFIED Requirements

### Requirement: Sandboxed Luau environment
Scripts SHALL execute in a sandboxed Luau environment exposing only: `string`, `table`, `math`, `utf8`, `bit32`, `buffer`, `os.time`, `os.clock`, `print`, and a restricted `coroutine` table containing only `yield` (retained because the embedded async runtime requires it; all coroutine scheduling primitives MUST remain absent). The ambient environment MUST NOT expose file I/O, network, or debug facilities, and MUST NOT expose subprocess execution as a global (no `os.execute`, no `io`). Process execution reaches scripts only through the injected `ptah.exec` capability, specified by the `shell-exec` capability; scripts have no other host filesystem or network access beyond driving agents and `ptah.exec`.

#### Scenario: Sandboxed globals
- **WHEN** a script accesses `io`, `os.execute`, `debug`, or `coroutine.create`
- **THEN** the access resolves to nil (or raises an error on call) because the globals are absent

#### Scenario: Print passthrough
- **WHEN** a script calls `print("hello")`
- **THEN** the line is written to ptah's standard output unmodified, without session prefixes

### Requirement: Agent and session API
The `ptah` namespace SHALL provide `ptah.agent(name_or_spec)` returning an agent factory, and `agent:session(options)` returning a session object. Each `session()` call creates an independent session with its own agent subprocess. Session options SHALL accept `cwd` (resolved relative to the invocation directory), `id` (label used in output attribution, defaulting to `s1`, `s2`, … per agent), `mcpServers`, and `resultSchema` (a JSON Schema expressed as a Luau table; the option's semantics are specified by the typed-results capability). Two `ptah.agent` calls for the same name SHALL return independent factory objects.

#### Scenario: Session creation
- **WHEN** a script calls `ptah.agent("claude"):session({ id = "reviewer" })`
- **THEN** a session labeled `claude/reviewer` exists and is ready to prompt

#### Scenario: Default session labels
- **WHEN** two sessions are created without `id` from the same agent factory
- **THEN** they are labeled `s1` and `s2` respectively in output attribution

#### Scenario: Independent factories
- **WHEN** `ptah.agent("claude")` is called twice with the same name and each factory creates a session
- **THEN** the factories keep independent session counters: both first sessions are labeled `claude/s1`

#### Scenario: Unknown agent name
- **WHEN** `ptah.agent("nope")` is called and `nope` exists in no registry
- **THEN** a Lua error is raised naming the unresolved agent

### Requirement: Task and concurrency primitives
The `ptah` namespace SHALL provide: `ptah.spawn(fn)` returning a Task object with `:await()`, `ptah.join({task, ...})` waiting for all tasks, `ptah.parallel(items, fn, options?)` running `fn` per item with optional `concurrency` limit (default unlimited) and returning per-item outcome entries, and `ptah.sleep(ms)`. Awaiting an errored task SHALL re-raise its error at the await site. `ptah.parallel` results SHALL carry each item's success value or error without throwing wholesale. A task error is delivered when observed via `:await()`, `join`, or carried in `ptah.parallel` results, whether or not the script catches or inspects it; a task error never delivered by script end SHALL fail the run (error to stderr, non-zero exit).

#### Scenario: Parallel fan-out
- **WHEN** `ptah.parallel({1,2,3}, function(i) return agent:session():prompt("q"..i) end)` runs
- **THEN** all three turns execute concurrently, each on its own session, and results arrive in item order

#### Scenario: Concurrency cap
- **WHEN** `ptah.parallel(items, fn, { concurrency = 2 })` runs with 5 items
- **THEN** at most 2 `fn` invocations are in flight simultaneously

#### Scenario: Contained task error
- **WHEN** one of three spawned tasks raises and the script joins all three
- **THEN** the two successful values are available and the failed task's error is reported for its entry; other tasks are unaffected

#### Scenario: Error re-raised at await
- **WHEN** a spawned function raises an error and `task:await()` is called
- **THEN** the original error is re-raised at the await call site

### Requirement: Runtime helpers
The `ptah` namespace SHALL provide `ptah.log(msg)` printing a `[ptah]`-prefixed diagnostic line to standard output, `ptah.exit(code)` terminating the run, `ptah.sleep(ms)` yielding the current task for the duration, and `ptah.version` (read-only version string).

#### Scenario: Log attribution
- **WHEN** a script calls `ptah.log("starting")`
- **THEN** output shows `[ptah] starting` on its own line

#### Scenario: Sleep yields
- **WHEN** task A calls `ptah.sleep(100)` while task B prompts
- **THEN** task B progresses during A's sleep

### Requirement: JSON module
The `ptah` namespace SHALL provide `ptah.json.parse(string)` returning the decoded value as Luau data (arrays as tables with consecutive integer keys starting at 1, objects as string-keyed tables, `null` as `nil`), raising a Lua error on malformed input; and `ptah.json.stringify(value, { indent?: number })` returning the encoded JSON string. The module performs no I/O.

#### Scenario: Round trip
- **WHEN** a script calls `ptah.json.parse('{"a":[1,2]}')` and stringifies the result with `ptah.json.stringify(v, { indent = 2 })`
- **THEN** the output is valid JSON encoding `{"a": [1, 2]}` with two-space indentation

#### Scenario: Malformed input raises
- **WHEN** a script calls `ptah.json.parse("{oops")`
- **THEN** a Lua error is raised and can be caught with `pcall`

#### Scenario: Decoding command output
- **WHEN** a script calls `ptah.exec("printf '[{\\"n\\":1}]'")` and passes `r.stdout` to `ptah.json.parse`
- **THEN** the result is a table whose first element is `{ n = 1 }`
