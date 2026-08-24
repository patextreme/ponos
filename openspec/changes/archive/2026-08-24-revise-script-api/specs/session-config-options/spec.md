## ADDED Requirements

### Requirement: Config applied at session creation
`agent:session(options)` SHALL accept a `config` option: a Luau table whose keys are config-option ids and whose values are strings (select value ids) or booleans. After the agent session is created and before the `session()` call returns, ponos SHALL apply each entry as one `session/set_config_option` request, identical in wire form and value typing to a `setConfig` call. The application order across entries SHALL be unspecified: scripts MUST NOT rely on any ordering. ponos SHALL NOT validate keys or values locally against the session's advertised option state; the agent's response is authoritative.

#### Scenario: Model pinned at creation
- **WHEN** `agent:session({ config = { model = "opus" } })` is called and the agent accepts the value
- **THEN** the constructor returns a session whose `model` option is set, and the first prompt runs under that setting

#### Scenario: Multiple options at once
- **WHEN** `agent:session({ config = { model = "haiku", verbose = true } })` is called and the agent accepts both
- **THEN** both options are set on the session before the constructor returns, regardless of the order in which the entries are applied

#### Scenario: Agent rejection fails the constructor
- **WHEN** `agent:session({ config = { model = "no-such-model" } })` is called and the agent rejects the value
- **THEN** the `session()` call raises a catchable Lua error carrying the config id and the agent's message, and the spawned agent subprocess is torn down

#### Scenario: Non-string-or-boolean value fails before spawn
- **WHEN** `agent:session({ config = { model = 42 } })` is called
- **THEN** the `session()` call raises a Lua error naming the invalid entry, before any agent subprocess is spawned

#### Scenario: Config composes with later setConfig
- **WHEN** a session is created with `config = { model = "opus" }` and the script later calls `s:setConfig("model", "haiku")`
- **THEN** the later call applies as usual and the prompt-outcome option state reflects the new value
