## MODIFIED Requirements

### Requirement: Config option capability is advertised
ptah SHALL advertise the `session.configOptions` client capability during `initialize` so capability-gating agents may include config options in their `session/new` responses, including its `boolean` sub-capability so agents may offer boolean options and accept boolean `set_config_option` values. No other client capability SHALL be declared.

#### Scenario: Capability present in handshake
- **WHEN** ptah performs the `initialize` handshake with an agent
- **THEN** the request's client capabilities include `session.configOptions` with its `boolean` sub-capability and no interactive capability

### Requirement: Option state is captured and kept live
ptah SHALL capture the `configOptions` array from each `session/new` response as that session's initial option state, and SHALL update it from `config_option_update` notifications and `session/set_config_option` responses so the surfaced state never goes stale.

#### Scenario: Options captured at session start
- **WHEN** a session is created on an agent that returns config options
- **THEN** `session:configOptions()` reports them with their advertised current values

#### Scenario: Agent-pushed update is folded
- **WHEN** the agent sends a `session/update` carrying `config_option_update` mid-session
- **THEN** subsequent `session:configOptions()` calls report the new option state
