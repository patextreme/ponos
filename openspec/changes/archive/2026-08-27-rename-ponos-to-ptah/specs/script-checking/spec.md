## MODIFIED Requirements

### Requirement: Check subcommand verifies a script without execution
The CLI SHALL provide `ptah check <script.luau>` taking exactly one positional path to the entry Luau script. Checking SHALL NOT execute any script code: the entry chunk is compiled but never called, no required module runs, no agent subprocess is launched, and no renderer output is produced.

#### Scenario: Clean script
- **WHEN** `ptah check script.luau` is invoked on a script that passes all passes
- **THEN** the process exits with code 0

#### Scenario: No execution side effects
- **WHEN** a checked script's top level contains calls that would spawn agents, prompt, or print
- **THEN** checking launches no agent subprocess and produces no script output

#### Scenario: Missing script argument
- **WHEN** `ptah check` is invoked without a positional path
- **THEN** the CLI prints a usage error and exits 2

### Requirement: Static lints walk the literal require graph
The check SHALL statically analyze the entry and every file reachable through literal string `require("...")` call arguments, resolving each path relative to its requiring file under ptah's module-resolution rules (no boundary: requires may traverse out of the entry script's directory), without executing anything. It SHALL report:

- **Unknown agent names**: a literal `ptah.agent("<name>")` string argument that resolves in no discovered registry (project `.ptah/config.toml` found upward from the invocation directory overriding the user config per agent name, exactly as `run` discovers) is a finding. Non-literal (computed) arguments and inline spec tables SHALL NOT be linted.
- **Broken requires**: a literal require target that does not resolve to an existing module file (`.luau`, `.lua`, `init.luau`, `init.lua`) is a finding. A require whose target exists outside the entry script's directory is NOT a finding.
- **Missing strict directive**: the entry and every reachable file SHALL begin with a `--!strict` directive; a file without it is a finding.

#### Scenario: Unknown literal agent name
- **WHEN** a reachable file contains `ptah.agent("clawed")` and no registry defines `clawed`
- **THEN** the check reports a finding naming the agent and exits 1

#### Scenario: Computed agent name is not linted
- **WHEN** a reachable file contains `ptah.agent(name)` where `name` is a variable
- **THEN** the check reports no finding for that call

#### Scenario: Require outside the entry tree is not a finding
- **WHEN** a reachable file contains `require("../../outside")` and the target resolves to an existing module file outside the entry script's directory
- **THEN** the check reports no finding for that require

#### Scenario: Missing module
- **WHEN** a reachable file contains `require("./lib/nope")` and no such module file exists
- **THEN** the check reports a finding naming the unresolved path

#### Scenario: Missing strict directive in a module
- **WHEN** the entry declares `--!strict` but a reachable required module does not
- **THEN** the check reports a finding naming the module file and exits 1

#### Scenario: Registry agent resolves
- **WHEN** a reachable file contains `ptah.agent("claude")` and any discovered registry defines `claude`
- **THEN** the check reports no finding for that call

### Requirement: Typecheck pass runs luau-lsp with the embedded definitions
The check SHALL invoke the `luau-lsp` binary discovered on PATH as `luau-lsp analyze` with the standard platform and a definitions file derived from the binary's embedded type definitions (written to a temporary location). luau-lsp's stderr SHALL pass through unmodified and unfiltered; a non-zero luau-lsp exit status SHALL make the check report findings (exit 1), and a zero exit status contributes no findings.

#### Scenario: Type error caught by strict analysis
- **WHEN** a `--!strict` script contains a member typo (e.g. `agent:sesion(...)`)
- **THEN** luau-lsp's diagnostic output is passed through and the check exits 1

#### Scenario: Warnings do not fail
- **WHEN** luau-lsp reports only warnings (e.g. `LocalUnused`) and exits 0
- **THEN** the check does not treat them as findings

#### Scenario: luau-lsp missing from PATH
- **WHEN** `ptah check` runs and no `luau-lsp` executable is on PATH
- **THEN** the check prints an error naming the missing dependency and exits 2; no silent skip occurs
