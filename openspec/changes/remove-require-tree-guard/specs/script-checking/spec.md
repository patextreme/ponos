## MODIFIED Requirements

### Requirement: Static lints walk the literal require graph
The check SHALL statically analyze the entry and every file reachable through literal string `require("...")` call arguments, resolving each path relative to its requiring file under ponos's module-resolution rules (no boundary: requires may traverse out of the entry script's directory), without executing anything. It SHALL report:

- **Unknown agent names**: a literal `ponos.agent("<name>")` string argument that resolves in no discovered registry (project `.ponos/config.toml` found upward from the invocation directory overriding the user config per agent name, exactly as `run` discovers) is a finding. Non-literal (computed) arguments and inline spec tables SHALL NOT be linted.
- **Broken requires**: a literal require target that does not resolve to an existing module file (`.luau`, `.lua`, `init.luau`, `init.lua`) is a finding. A require whose target exists outside the entry script's directory is NOT a finding.
- **Missing strict directive**: the entry and every reachable file SHALL begin with a `--!strict` directive; a file without it is a finding.

#### Scenario: Unknown literal agent name
- **WHEN** a reachable file contains `ponos.agent("clawed")` and no registry defines `clawed`
- **THEN** the check reports a finding naming the agent and exits 1

#### Scenario: Computed agent name is not linted
- **WHEN** a reachable file contains `ponos.agent(name)` where `name` is a variable
- **THEN** the check reports no finding for that call

#### Scenario: Require escaping the script tree
- **WHEN** a reachable file contains `require("../../outside")` and the target resolves to an existing module file outside the entry script's directory
- **THEN** the check reports no finding for that require

#### Scenario: Missing module
- **WHEN** a reachable file contains `require("./lib/nope")` and no such module file exists
- **THEN** the check reports a finding naming the unresolved path

#### Scenario: Missing strict directive in a module
- **WHEN** the entry declares `--!strict` but a reachable required module does not
- **THEN** the check reports a finding naming the module file and exits 1

#### Scenario: Registry agent resolves
- **WHEN** a reachable file contains `ponos.agent("claude")` and any discovered registry defines `claude`
- **THEN** the check reports no finding for that call
