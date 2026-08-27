# CLI Specification

## Purpose

Defines the user-facing command surface of the ptah binary: how scripts are invoked, how output is controlled, and how the process exits.

## Requirements

### Requirement: Run subcommand executes a script
The `ptah` CLI SHALL provide `ptah run <script.luau>`, where `<script.luau>` is a positional required path to the entry Luau script.

#### Scenario: Successful run
- **WHEN** `ptah run script.luau` is invoked and the script completes without uncaught errors
- **THEN** the process exits with code 0

#### Scenario: Missing script argument
- **WHEN** `ptah run` is invoked without a positional path
- **THEN** the CLI prints a usage error and exits non-zero without executing anything

#### Scenario: Nonexistent script file
- **WHEN** the positional path does not exist on disk
- **THEN** the CLI prints an error naming the path and exits non-zero

### Requirement: Run pre-flight fails certain-broken scripts before spawning
`ptah run` SHALL perform an in-process pre-flight before executing the script: compile/parse the entry and every file reachable through literal `require("...")` string arguments, resolve literal require targets under ptah's module-resolution rules (existence; no boundary — requires may traverse out of the entry script's directory), and resolve literal `ptah.agent("<name>")` string arguments against the discovered registry. A pre-flight failure SHALL fail the run before any agent subprocess spawns, with the finding(s) printed to standard error and exit code 1.

The pre-flight SHALL NOT execute script code, SHALL NOT enforce the `--!strict` directive, and SHALL NOT invoke `luau-lsp`. Non-literal (computed) require paths and agent names SHALL NOT be pre-flighted — a script using them runs exactly as before.

#### Scenario: Unknown literal agent name fails fast
- **WHEN** `ptah run script.luau` runs and the script contains `ptah.agent("clawed")` where no registry defines `clawed`
- **THEN** the run fails before any agent subprocess spawns and exits 1

#### Scenario: Broken literal require fails fast
- **WHEN** a script contains `require("./lib/missing")` and no such module file exists
- **THEN** the run fails immediately with a finding naming the unresolved path, before any agent spawns

#### Scenario: Cross-tree require passes pre-flight
- **WHEN** a script contains `require("../shared/util")` and the module exists outside the entry script's directory
- **THEN** the pre-flight resolves it without findings and the run proceeds

#### Scenario: Non-strict scripts still run
- **WHEN** a script without a `--!strict` directive is run
- **THEN** the run proceeds exactly as before; the directive is not required for execution

#### Scenario: Computed agent name is not pre-flighted
- **WHEN** a script calls `ptah.agent(name)` with a variable
- **THEN** the pre-flight makes no claim about it and the run proceeds

#### Scenario: Unreachable missing require is accepted risk
- **WHEN** a script requires a missing module on a code path that never executes at runtime
- **THEN** the pre-flight still fails the run (documented, accepted false-positive class)

### Requirement: Output control flags
The CLI SHALL accept output flags: `--quiet` suppresses all streaming render and diagnostics, `--verbose` shows runtime lifecycle diagnostics, a second verbosity level (`-vv`) additionally passes agent subprocess stderr through, and `--no-color` disables ANSI colors while keeping text prefixes.

#### Scenario: Quiet flag
- **WHEN** a script runs with `--quiet`
- **THEN** no streaming output from agents is printed (output produced by the script itself via `print` still passes through)

#### Scenario: No-color degradation
- **WHEN** a script runs with `--no-color`
- **THEN** session output is still attributed by its text prefix but contains no ANSI escape sequences

### Requirement: Rendered lines are timestamped
Every rendered output line — agent message chunks, tool lines, plan summaries, context-usage lines, lifecycle diagnostics, `ptah.log` lines, and `-vv` agent stderr passthrough — SHALL be prefixed with a local-time timestamp shaped `yyyy-mm-dd HH:MM:SS` (space-separated), per the `render-logging` capability's timestamp contract, ahead of the session attribution prefix. Timestamps SHALL be always on: no flag controls them. `--no-color` SHALL keep the timestamp as plain text, and `--quiet` SHALL continue to suppress all rendered output. Script `print` output does not pass through the renderer and SHALL NOT be timestamped or otherwise modified.

#### Scenario: Timestamp on rendered lines
- **WHEN** any rendered line is emitted (message chunk, tool line, plan, usage, lifecycle diagnostic, `ptah.log`, or agent stderr passthrough)
- **THEN** the line begins with a `yyyy-mm-dd HH:MM:SS` local-time timestamp

#### Scenario: No-color keeps plain timestamps
- **WHEN** a script runs with `--no-color`
- **THEN** rendered lines still carry the timestamp as plain text, without ANSI sequences

#### Scenario: Script print output is untouched
- **WHEN** a script calls `print("hello")`
- **THEN** the output line is exactly the script's text with no timestamp or prefix added

### Requirement: Version flag
The CLI SHALL support `--version`, printing the ptah version string and exiting 0.

#### Scenario: Print version
- **WHEN** `ptah --version` is invoked
- **THEN** the version string is printed and the process exits 0

### Requirement: Script end waits for outstanding tasks
WHEN the main script chunk finishes while spawned tasks are still running, the process SHALL wait for all outstanding tasks to complete before exiting, UNLESS the script calls `ptah.exit(code)`.

#### Scenario: Pending spawn at script end
- **WHEN** the main chunk returns while a `ptah.spawn` task is still awaiting an agent prompt
- **THEN** ptah waits for that task to finish before exiting

#### Scenario: Explicit exit
- **WHEN** the script calls `ptah.exit(3)` while tasks are pending
- **THEN** pending tasks and agent processes are torn down and the process exits with code 3

### Requirement: Uncaught script error fails the run
WHEN a script error escapes the main chunk uncaught, ptah SHALL cancel all in-flight prompt turns, terminate all agent subprocesses, print the error to stderr, and exit with a non-zero code. A task error that is never delivered to the script is treated the same way at script end: after all outstanding tasks complete, any task whose error was never observed (via `:await()`, `join`, or as a value in `ptah.parallel` results) SHALL fail the run with that error printed to stderr and a non-zero exit code — unless the script already terminated via `ptah.exit`, whose code wins.

#### Scenario: Error propagation
- **WHEN** a spawned task raises and the script awaits it without catching
- **THEN** the error is re-raised at the await site; if it escapes the main chunk uncaught, the run terminates non-zero

#### Scenario: Never-retrieved task error at script end
- **WHEN** a spawned task raises an error the script never observes via `await`/`join`, and the main chunk finishes normally
- **THEN** ptah waits for outstanding tasks, prints the task's error to stderr, and exits non-zero

### Requirement: Session cwd defaults to invocation directory
The default working directory for agent sessions SHALL be the directory from which `ptah run` was invoked, unless the script overrides it per session.

#### Scenario: Default cwd
- **WHEN** a session is created without an explicit `cwd`
- **THEN** the session's working directory is ptah's invocation directory
