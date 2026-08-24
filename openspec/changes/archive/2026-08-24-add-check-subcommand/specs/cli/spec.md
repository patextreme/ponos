# CLI Spec Delta

## ADDED Requirements

### Requirement: Run pre-flight fails certain-broken scripts before spawning
`ponos run` SHALL perform an in-process pre-flight before executing the script: compile/parse the entry and every file reachable through literal `require("...")` string arguments, resolve literal require targets under ponos's module-resolution rules (existence and script-tree escape guard), and resolve literal `ponos.agent("<name>")` string arguments against the discovered registry. A pre-flight failure SHALL fail the run before any agent subprocess spawns, with the finding(s) printed to standard error and exit code 1.

The pre-flight SHALL NOT execute script code, SHALL NOT enforce the `--!strict` directive, and SHALL NOT invoke `luau-lsp`. Non-literal (computed) require paths and agent names SHALL NOT be pre-flighted — a script using them runs exactly as before.

#### Scenario: Unknown literal agent name fails fast
- **WHEN** `ponos run script.luau` runs and the script contains `ponos.agent("clawed")` where no registry defines `clawed`
- **THEN** the run fails before any agent subprocess spawns and exits 1

#### Scenario: Broken literal require fails fast
- **WHEN** a script contains `require("./lib/missing")` and no such module file exists
- **THEN** the run fails immediately with a finding naming the unresolved path, before any agent spawns

#### Scenario: Non-strict scripts still run
- **WHEN** a script without a `--!strict` directive is run
- **THEN** the run proceeds exactly as before; the directive is not required for execution

#### Scenario: Computed agent name is not pre-flighted
- **WHEN** a script calls `ponos.agent(name)` with a variable
- **THEN** the pre-flight makes no claim about it and the run proceeds

#### Scenario: Unreachable missing require is accepted risk
- **WHEN** a script requires a missing module on a code path that never executes at runtime
- **THEN** the pre-flight still fails the run (documented, accepted false-positive class)
