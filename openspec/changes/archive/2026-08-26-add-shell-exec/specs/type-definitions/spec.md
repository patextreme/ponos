## MODIFIED Requirements

### Requirement: Definitions cover the script API
A definitions file SHALL declare the `ponos` global with its full public surface: `agent`, `spawn`, `parallel`, `join`, `sleep`, `log`, `exit`, and `version`; session objects (`prompt` returning a result table with `text`, `stopReason`, a `usage` table of `input`/`cacheRead`/`cacheWrite`/`output`, and `result` holding the turn's typed-result value (`nil` when there was no accepted submission), `cancel`, `label`, `close`, `configOptions`, and `setConfig`); task objects (`await`); session and task option tables; and agent spec tables. Outcome entries SHALL be typed as a discriminated union of `{ ok: true, value: T } | { ok: false, error: string }`, and `parallel`/`spawn` SHALL be generic so result types propagate. The `mcpServers` option SHALL be typed after the session-configuration structure the runtime accepts, not left untyped; the `resultSchema` session option SHALL be typed as an optional string-keyed table carrying the declared JSON Schema and the prompt-result `result` field as an optional field for the converted submission value. The `SessionOptions` type SHALL NOT declare a `config` field (the option is removed; scripts apply config with `setConfig` after session creation). The config-option surface SHALL be typed: `configOptions()` returning an array of option entries (`id`, `name`, `type`, `currentValue: string | boolean`, optional `category`, and an `options` choice array for select options) and `setConfig(id: string, value: string | boolean)`.

The definitions SHALL additionally type `exec`: `ponos.exec(cmd: string, opts?: { timeoutMs: number? }) -> ExecResult` where `ExecResult` is `{ exitCode: number, stdout: string, stderr: string }`. The definitions SHALL additionally type the `json` module: `ponos.json.parse(s: string) -> any` (raising on malformed input) and `ponos.json.stringify(value: any, opts?: { indent: number? }) -> string`.

#### Scenario: Typo in result field
- **WHEN** a script analyzed with the definitions accesses an invented field on a prompt result (e.g. `r.txt`)
- **THEN** analysis reports a type error naming the result table type

#### Scenario: Outcome narrowing
- **WHEN** a script binds a `ponos.parallel` result to a local and branches on `entry.ok`
- **THEN** analysis narrows the local to the `value` field on the true branch and the `error` field on the false branch

#### Scenario: Typed-result surface type-checks
- **WHEN** a strict-mode script analyzed with the definitions passes `resultSchema = { type = "object" }` in `agent:session(…)` options and reads `r.result` on a prompt outcome
- **THEN** analysis accepts both uses, while an invented outcome field (e.g. `r.txt`) still reports a type error naming the result table type (excess keys in option table literals are a known analyzer residual, documented in the README)

#### Scenario: Constructor config type-checks
- **WHEN** a strict-mode script analyzed with the definitions passes `config = { model = "opus" }` in `agent:session(…)` options
- **THEN** the `SessionOptions` type declares no `config` field (excess keys in option table literals are a known analyzer residual, documented in the README), and running the script raises the pre-spawn rejection error instead

#### Scenario: Wrong setConfig value type
- **WHEN** a script analyzed with the definitions calls `s:setConfig("model", 42)`
- **THEN** analysis reports a type error on the value argument

#### Scenario: Exec result fields type-check
- **WHEN** a script analyzed with the definitions binds `local r = ponos.exec("true")` and reads `r.exitCode`, `r.stdout`, `r.stderr`, then reads `r.out`
- **THEN** the first three reads are accepted and `r.out` reports a type error naming the exec result type

#### Scenario: Exec options type-check
- **WHEN** a strict-mode script analyzed with the definitions calls `ponos.exec("true", { timeoutMs = 100 })` and separately `ponos.exec(cmd, 100)`
- **THEN** the options-table call is accepted and the bare-number call reports a type error

#### Scenario: JSON module type-checks
- **WHEN** a script analyzed with the definitions calls `ponos.json.parse(s).x` and `ponos.json.stringify(v, { indent = 2 })`
- **THEN** both calls are accepted, and a call to an invented member (e.g. `ponos.json.load`) reports a type error
