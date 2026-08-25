## REMOVED Requirements

### Requirement: Config applied at session creation
**Reason**: the `config` table's unspecified `pairs()` iteration order is
load-bearing for agents with dependent options (opencode re-derives `effort`
from the model on every `model` set, silently reverting an effort applied
earlier in the same table), and a string-keyed Luau table cannot express
ordering — so the option cannot be made correct without changing its shape.

**Migration**: apply config with `setConfig` calls after `session()` returns,
in dependency order — driving options (e.g. `model`) first, dependent options
(e.g. `effort`) last. A rejected `setConfig` raises a catchable error naming
the config id and the agent's message; the session stays open until the script
closes it or the run-end sweep reaps it (no constructor atomicity).

## ADDED Requirements

### Requirement: Constructor config key is rejected
`agent:session(options)` SHALL reject a `config` key in the options table
(whether populated or empty) by raising a catchable Lua error before any
agent subprocess is spawned. The error message SHALL name the removed `config`
option and direct the author to `setConfig` calls after session creation,
including the sequencing guidance to set driving options (e.g. `model`) first
when the agent has dependent options. No other unknown option key SHALL be
rejected by this rule.

#### Scenario: Config key errors before spawn
- **WHEN** `agent:session({ config = { model = "opus" } })` is called
- **THEN** the `session()` call raises a catchable Lua error mentioning `config` and `setConfig`, and no agent subprocess is spawned

#### Scenario: Empty config table errors identically
- **WHEN** `agent:session({ config = {} })` is called
- **THEN** the `session()` call raises the same rejection error as a populated table

## MODIFIED Requirements

### Requirement: setConfig changes options between turns
Session objects SHALL provide `setConfig(id, value)` accepting a string (select value id) or boolean value; any other Luau type SHALL raise a Lua error before anything is sent. `setConfig` SHALL be serialized with prompt turns on the same session: a call issued while a turn is in flight waits for that turn to complete, so config changes apply strictly between turns. Sequential `setConfig` calls SHALL be applied in the order the script awaits them, making dependent-option sequencing script-controlled: the agent's response state (including any dependent option the agent re-derives) is authoritative after each call. On agent rejection or unsupported-method error, `setConfig` SHALL raise a Lua error carrying the agent's message; on success it SHALL update the session's option state from the response and return nil.

#### Scenario: Switching model before first prompt
- **WHEN** `s:setConfig("model", "claude-haiku-4-5")` succeeds on a fresh session
- **THEN** the session's option state reports `model` currentValue `claude-haiku-4-5` and subsequent prompts run under it

#### Scenario: Mid-turn setConfig waits
- **WHEN** `setConfig` is called while a prompt turn is in flight on the same session
- **THEN** the `session/set_config_option` request is sent only after the in-flight turn completes

#### Scenario: Sequencing is script-controlled
- **WHEN** a script awaits `s:setConfig("model", "m1")` and then `s:setConfig("effort", "high")` on an agent that re-derives `effort` when `model` is set
- **THEN** the `effort` set is applied after the `model` set and the session's final option state reports `effort` as `high`

#### Scenario: Agent rejects the value
- **WHEN** the agent returns an error for `session/set_config_option`
- **THEN** `setConfig` raises a catchable Lua error naming the config id and carrying the agent's message

#### Scenario: Unsupported method
- **WHEN** the agent answers `session/set_config_option` with a method-not-found error
- **THEN** `setConfig` raises a catchable Lua error rather than silently succeeding
