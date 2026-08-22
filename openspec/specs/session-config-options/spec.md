# Session Config Options Specification

## Purpose

Defines the per-session configuration surface exposed to scripts: discovery of agent-offered config options (model above all), mutation between turns, live state tracking, and the `session.configOptions` client capability that unlocks them.

## Requirements

### Requirement: Config option capability is advertised
ponos SHALL advertise the `session.configOptions` client capability during `initialize` so capability-gating agents may include config options in their `session/new` responses, including its `boolean` sub-capability so agents may offer boolean options and accept boolean `set_config_option` values. No other client capability SHALL be declared.

#### Scenario: Capability present in handshake
- **WHEN** ponos performs the `initialize` handshake with an agent
- **THEN** the request's client capabilities include `session.configOptions` with its `boolean` sub-capability and no interactive capability

### Requirement: Option state is captured and kept live
ponos SHALL capture the `configOptions` array from each `session/new` response as that session's initial option state, and SHALL update it from `config_option_update` notifications and `session/set_config_option` responses so the surfaced state never goes stale.

#### Scenario: Options captured at session start
- **WHEN** a session is created on an agent that returns config options
- **THEN** `session:configOptions()` reports them with their advertised current values

#### Scenario: Agent-pushed update is folded
- **WHEN** the agent sends a `session/update` carrying `config_option_update` mid-session
- **THEN** subsequent `session:configOptions()` calls report the new option state

### Requirement: Session API exposes config options
Session objects SHALL provide `configOptions()` returning the session's live option state as an array (empty when the agent offers none). Each entry SHALL carry `id`, `name`, `type` (`"select"` or `"boolean"`), `currentValue` (string or boolean), optional `category` (nil when the agent omits it), and — for select options — an `options` array of `{ id, name, description? }` choices.

#### Scenario: Reading a model option
- **WHEN** the agent advertises a select option with id `model` and currentValue `claude-opus-4-5`
- **THEN** `s:configOptions()` contains an entry with `id == "model"`, `type == "select"`, `currentValue == "claude-opus-4-5"`, and an `options` list of choices

#### Scenario: Agent without options
- **WHEN** the agent returns no config options
- **THEN** `s:configOptions()` returns an empty table

### Requirement: setConfig changes options between turns
Session objects SHALL provide `setConfig(id, value)` accepting a string (select value id) or boolean value; any other Luau type SHALL raise a Lua error before anything is sent. `setConfig` SHALL be serialized with prompt turns on the same session: a call issued while a turn is in flight waits for that turn to complete, so config changes apply strictly between turns. On agent rejection or unsupported-method error, `setConfig` SHALL raise a Lua error carrying the agent's message; on success it SHALL update the session's option state from the response and return nil.

#### Scenario: Switching model before first prompt
- **WHEN** `s:setConfig("model", "claude-haiku-4-5")` succeeds on a fresh session
- **THEN** the session's option state reports `model` currentValue `claude-haiku-4-5` and subsequent prompts run under it

#### Scenario: Mid-turn setConfig waits
- **WHEN** `setConfig` is called while a prompt turn is in flight on the same session
- **THEN** the `session/set_config_option` request is sent only after the in-flight turn completes

#### Scenario: Agent rejects the value
- **WHEN** the agent returns an error for `session/set_config_option`
- **THEN** `setConfig` raises a catchable Lua error naming the config id and carrying the agent's message

#### Scenario: Unsupported method
- **WHEN** the agent answers `session/set_config_option` with a method-not-found error
- **THEN** `setConfig` raises a catchable Lua error rather than silently succeeding

### Requirement: Config changes are rendered
Successful `setConfig` calls and agent-pushed `config_option_update` changes SHALL each render one session-attributed lifecycle line naming each changed option id and its new value. An agent-pushed update arriving with no prior option state SHALL render every advertised option as changed.

#### Scenario: Lifecycle line on set
- **WHEN** `s:setConfig("model", "opus")` succeeds
- **THEN** the renderer emits a lifecycle line for that session naming `model` and its new value

#### Scenario: Lifecycle line on agent-pushed change
- **WHEN** the agent pushes a `config_option_update` changing the `model` option mid-session
- **THEN** the renderer emits a lifecycle line for that session naming `model` and its new value
