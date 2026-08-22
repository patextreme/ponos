## MODIFIED Requirements

### Requirement: Definitions cover the script API
A definitions file SHALL declare the `ponos` global with its full public surface: `agent`, `spawn`, `map`, `join`, `sleep`, `log`, `exit`, and `version`; session objects (`prompt` returning a result table with `text`, `stopReason`, a `usage` table of `input`/`cacheRead`/`cacheWrite`/`output`, and `result` holding the turn's typed-result value (`nil` when there was no accepted submission), `cancel`, `label`, `close`, `configOptions`, and `setConfig`); task objects (`await`); session and task option tables; and agent spec tables. Outcome entries SHALL be typed as a discriminated union of `{ ok: true, value: T } | { ok: false, error: string }`, and `map`/`spawn` SHALL be generic so result types propagate. The `mcpServers` option SHALL be typed after the session-configuration structure the runtime accepts, not left untyped; the `result` session option SHALL be typed as an optional string-keyed table carrying the declared JSON Schema, and the prompt-result `result` field as an optional field for the converted submission value. The config-option surface SHALL be typed: `configOptions()` returning an array of option entries (`id`, `name`, `type`, `currentValue: string | boolean`, optional `category`, and an `options` choice array for select options) and `setConfig(id: string, value: string | boolean)`.

#### Scenario: Typo in result field
- **WHEN** a script analyzed with the definitions accesses an invented field on a prompt result (e.g. `r.txt`)
- **THEN** analysis reports a type error naming the result table type

#### Scenario: Outcome narrowing
- **WHEN** a script binds a `ponos.map` result to a local and branches on `entry.ok`
- **THEN** analysis narrows the local to the `value` field on the true branch and the `error` field on the false branch

#### Scenario: Typed-result surface type-checks
- **WHEN** a strict-mode script analyzed with the definitions passes `result = { type = "object" }` in `agent:session(…)` options and reads `r.result` on a prompt outcome
- **THEN** analysis accepts both uses, while an invented option or outcome field still reports a type error

#### Scenario: Wrong setConfig value type
- **WHEN** a script analyzed with the definitions calls `s:setConfig("model", 42)`
- **THEN** analysis reports a type error on the value argument
