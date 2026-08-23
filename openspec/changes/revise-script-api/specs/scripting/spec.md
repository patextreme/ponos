## MODIFIED Requirements

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
