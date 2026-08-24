# Type Definitions Specification

## Purpose

Defines the Luau type definitions that describe the `ponos` script API and its sandboxed environment to editors and analyzers: their content contract, distribution via the `ponos types` subcommand, and the guards that keep them synchronized with the runtime.

## Requirements

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
- **THEN** analysis accepts both uses, while an invented outcome field (e.g. `r.txt`) still reports a type error naming the result table type (excess keys in option table literals are a known analyzer residual, documented in the README)

#### Scenario: Wrong setConfig value type
- **WHEN** a script analyzed with the definitions calls `s:setConfig("model", 42)`
- **THEN** analysis reports a type error on the value argument

### Requirement: Definitions model the sandbox
The definitions SHALL shadow the trimmed globals the runtime provides: `os` restricted to `time` and `clock`, `coroutine` restricted to `yield`, and `loadstring` and `collectgarbage` declared as nil.

#### Scenario: Removed global flagged
- **WHEN** a script analyzed with the definitions calls `os.date`, `coroutine.create`, or `loadstring`
- **THEN** analysis reports a type error instead of the call being accepted and failing at runtime

### Requirement: Types subcommand
The CLI SHALL provide `ponos types`, which prints the definitions to standard output prefixed with a generated header comment identifying the ponos version. The emitted definitions SHALL be byte-identical to the repository's definitions file apart from the header. The command SHALL exit 0 and not require a script, registry, or agent configuration.

#### Scenario: Emit definitions
- **WHEN** a user runs `ponos types`
- **THEN** the definitions are printed to stdout with a version header, suitable for redirection into a definitions file

#### Scenario: No side effects
- **WHEN** `ponos types` runs on a machine with no agent registry configured
- **THEN** it succeeds without spawning agents or reading script files

### Requirement: Definitions stay synchronized with the runtime
The repository SHALL include a runtime probe test that executes a script (against the mock agent) exercising every member, method, and field the definitions promise. The repository's check suite SHALL run static analysis over the bundled examples, the probe script, and script test fixtures using the definitions, in strict mode via per-file directives rather than a committed `.luaurc`.

#### Scenario: Defs promise a removed member
- **WHEN** a member documented in the definitions is removed or renamed in the runtime
- **THEN** the probe test fails

#### Scenario: Example regresses
- **WHEN** a bundled example or fixture contains a type error against the definitions
- **THEN** the static-analysis check fails

### Requirement: Editor setup documentation
The README SHALL document how to obtain definitions (`ponos types`) and the generic luau-lsp settings (VS Code and Neovim, standard platform) that load them, without the repository committing any editor or Luau configuration files. The documentation SHALL note the known residuals: strict analysis of generic `map` callbacks occasionally needs explicit parameter annotations; the prompt-result string-conversion sugar is not covered; outcome narrowing requires a local binding; the require-tree restriction is not enforced by editor analysis (luau-lsp resolves requires without ponos's escape-guard), though the runtime and `ponos check` both enforce it — the runtime at require time, `ponos check` statically before any run.

#### Scenario: Reader configures an editor
- **WHEN** a reader follows the README editor-setup section
- **THEN** they can produce a definitions file matching their installed ponos version and point luau-lsp at it using documented generic settings

#### Scenario: Reader understands the require-tree residual
- **WHEN** a reader encounters the residuals list in the editor-setup section
- **THEN** the require-tree entry states editor analysis does not enforce ponos's escape-guard and names `ponos check` as the static enforcement point
