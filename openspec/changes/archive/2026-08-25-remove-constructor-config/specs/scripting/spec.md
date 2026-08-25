## MODIFIED Requirements

### Requirement: Agent and session API
The `ponos` namespace SHALL provide `ponos.agent(name_or_spec)` returning an agent factory, and `agent:session(options)` returning a session object. Each `session()` call creates an independent session with its own agent subprocess. Session options SHALL accept `cwd` (resolved relative to the invocation directory), `id` (label used in output attribution, defaulting to `s1`, `s2`, … per agent), `mcpServers`, and `resultSchema` (a JSON Schema expressed as a Luau table; the option's semantics are specified by the typed-results capability). Two `ponos.agent` calls for the same name SHALL return independent factory objects.

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
